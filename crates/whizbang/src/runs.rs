//! `whizbang runs list|show`: finding and reading runs (`SPEC.md` section 3, "Metrics
//! from ordinary runs"; ADR 0005). Both only read, so both work while another process
//! writes the store.
//!
//! What a store holds is not trusted to be what this build would have written. A run's
//! id is whatever the store has it under, and is not held to the rule for the ids the
//! executor makes, or a run that `list` shows could not be shown. `show` takes a run by
//! its id, and as `list` prints it, escaped or cut, when that is no run's id ([`meant`]). Everything read from the store
//! is printed escaped, so nothing in it reaches a terminal as a control character,
//! except where the caller asks for bytes as they are stored (`--json`, `--diagnostic`).

use std::path::Path;
use std::sync::Arc;

use clap::Subcommand;
use whizbang_core::contract::Admission;
use whizbang_core::contract::record::{
    self, ArgumentValue, Handed, Judged, Lost, Paid, RunRecord, StoredRecordError,
};
use whizbang_core::contract::request::{StoredAdmissionError, admitted};
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Generation, Kind, Reader, Run};

use crate::store::Where;

#[derive(Subcommand)]
pub enum Command {
    /// List the runs the store holds, each with how it ended, its action, when it
    /// started and how long it took. A run that was admitted and has not ended is
    /// listed as such; one that has no record, or one this build cannot read, with the
    /// reason.
    List {
        #[command(flatten)]
        at: Where,
    },
    /// Show one run's record: what was asked, how it ended, and what it kept. A run
    /// that was admitted and has not ended is shown from its admission.
    Show {
        /// The run, by its id or as `runs list` prints it: a line's first field can be
        /// copied here, though it was escaped or cut.
        run: String,
        #[command(flatten)]
        at: Where,
        /// Print the record as it is stored: JSON, on one line.
        #[arg(long, conflicts_with = "diagnostic")]
        json: bool,
        /// Print what the run holds under one name, as it is stored: the bytes
        /// themselves, NOT escaped, so send them to a file or a pager rather than
        /// straight to a terminal. NAME is any diagnostic name of the run, as the record
        /// lists them (`journal`, `diagnostics/<name>`), or `record` for the record.
        #[arg(long, value_name = "NAME")]
        diagnostic: Option<String>,
    },
    /// Aggregate the runs a filter admits: how many; how they ended; for runs that
    /// judged, their verdicts and the unsure rate; retries and repairs; time as the
    /// median and the 95th percentile; cost; and the runs answered from the decision
    /// cache. The playground shows the same figures.
    Stats {
        #[command(flatten)]
        filter: whizbang::stats::Filter,
        #[command(flatten)]
        at: Where,
        /// Count the unsure verdicts by what made each unsure: `rule`, the rule of each
        /// answer whose row was unsure, with its disposition and confidence, by use case,
        /// read from the checklist each run of `check` kept. Aggregates only: no run and
        /// no item is named.
        #[arg(long, value_name = "BY", value_parser = ["rule"])]
        unsure_by: Option<String>,
        /// Count the rejected submissions of the runs of `check` under a use case, by
        /// class, and the runs by why each ended, read from what each run kept of them.
        /// Aggregates only: no run and no item is named, and nothing the judge wrote.
        #[arg(long)]
        rejections: bool,
    },
    /// Write the record of each run a filter admits, as it is stored: one JSON object a
    /// line, for analysis outside whizbang. A record holds no label of an evaluation set.
    Export {
        #[command(flatten)]
        filter: whizbang::stats::Filter,
        #[command(flatten)]
        at: Where,
    },
    /// Write a bundle: one run that ended, or a chain of them in the order given, with
    /// every run under each, their records, journals, diagnostics and the files of their
    /// inputs and outputs, as one file another store can import (`bundle import`). Each
    /// input a later run took from an earlier one's output is linked to it. It holds no
    /// path, no account and no value of the env file, and is not encrypted: it holds the
    /// pictures and words of the runs. Reads only: it works while another process writes.
    /// It writes only the file it is given, in a folder that is there: none over another
    /// unless `--overwrite` is given, none through a link, and none inside the store.
    Bundle {
        /// The runs, each a run nothing called, by its id or as `runs list` prints it:
        /// at most 16, in the order they ran.
        #[arg(required = true, value_name = "RUN")]
        runs: Vec<String>,
        /// The file to write, in a folder that is there.
        #[arg(long, value_name = "FILE")]
        to: std::path::PathBuf,
        /// Replace a file that is there already.
        #[arg(long)]
        overwrite: bool,
        #[command(flatten)]
        at: Where,
    },
}

/// Runs a runs command: the bytes to print on success, or the complaint.
pub fn run(command: &Command) -> Result<Vec<u8>, String> {
    match command {
        Command::List { at } => list(at.root()).map(String::into_bytes),
        Command::Show {
            run,
            at,
            json,
            diagnostic,
        } => show(at.root(), run, *json, diagnostic.as_deref()),
        Command::Stats {
            filter,
            at,
            unsure_by,
            rejections,
        } => {
            let figures = whizbang::stats::figures(at.root(), filter)?;
            let mut text = figures.text();
            if unsure_by.is_some() {
                text.push_str(&figures.unsure_text());
            }
            if *rejections {
                text.push_str(&whizbang::stats::rejections(at.root(), filter)?);
            }
            Ok(text.into_bytes())
        }
        Command::Export { filter, at } => whizbang::stats::export(at.root(), filter),
        Command::Bundle {
            runs,
            to,
            overwrite,
            at,
        } => bundle(at.root(), runs, to, *overwrite).map(String::into_bytes),
    }
}

/// Writes a bundle of these runs to `to`, as `store export` writes a file: what to print.
fn bundle(root: &Path, given: &[String], to: &Path, overwrite: bool) -> Result<String, String> {
    // Where it goes is held to its rules before anything is read.
    let (target, replace) = crate::export::destination(to, root, overwrite)?;
    let (_, generation) = open(root)?;
    let mut runs = Vec::new();
    for run in given {
        runs.push(meant(&generation, run)?.0.to_owned());
    }
    let runs: Vec<&str> = runs.iter().map(String::as_str).collect();
    let made = whizbang::bundling::make(root, &runs)?;
    crate::export::written(&target, replace, &made.bytes)?;
    let families: usize = made.manifest.steps.iter().map(|step| step.runs.len()).sum();
    Ok(format!(
        "bundled {} steps ({families} runs) as {} to {}\nfiles: {}\nwithheld: {}\n",
        made.manifest.steps.len(),
        made.digest,
        whizbang::door::printable(&to.display().to_string()),
        made.manifest.artifacts.len(),
        made.manifest.withheld.len()
    ))
}

/// How much of a text that was read from a store, or given on the command line, a
/// message shows: its first 64 characters, escaped.
fn shown(text: &str) -> String {
    let cut: String = text.chars().take(64).collect();
    let more = if text.chars().count() > 64 { "..." } else { "" };
    format!("{}{more}", cut.escape_debug())
}

/// A text the record's reader already bounds, whole and escaped: a model's names, at
/// most 128 ASCII characters each, and a device's, at most 32.
fn whole(text: &str) -> String {
    text.escape_debug().to_string()
}

/// A time in milliseconds since the Unix epoch, as a date and a time of day in UTC.
pub fn utc(ms: u64) -> String {
    let (days, in_day) = (ms / 86_400_000, ms % 86_400_000);
    // The civil date of a day number, by the usual arithmetic on eras of 400 years.
    let shifted = days + 719_468;
    let (era, day_of_era) = (shifted / 146_097, shifted % 146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        in_day / 3_600_000,
        in_day / 60_000 % 60,
        in_day / 1_000 % 60,
        in_day % 1_000
    )
}

fn open(root: &Path) -> Result<(Reader, Generation), String> {
    let reader = Reader::open(root, Arc::new(RealFs)).map_err(|e| e.to_string())?;
    let generation = reader.snapshot().map_err(|e| e.to_string())?;
    Ok((reader, generation))
}

fn record_of(
    reader: &Reader,
    generation: &Generation,
    id: &str,
) -> Option<Result<RunRecord, StoredRecordError>> {
    record::stored(generation, id, |entry| reader.read(entry))
}

fn list(root: &Path) -> Result<String, String> {
    let (reader, generation) = open(root)?;
    let mut text = format!("runs: {}\n", generation.runs().len());
    for id in generation.runs().keys() {
        let line = match record_of(&reader, &generation, id) {
            // What the record of a run a dead process left does not know is shown as
            // unknown: its action, when its admission could not be read, and its start.
            Some(Ok(record)) => {
                let unknown = || "unknown".to_owned();
                let (asked, time) = (record.asked(), record.time());
                format!(
                    "{}\t{}{}\t{} {}\t{}\t{}{}",
                    shown(id),
                    record.outcome().status.name(),
                    lost_in_a_line(&record),
                    asked.action.as_deref().map_or_else(unknown, shown),
                    asked.version.as_deref().map_or_else(unknown, shown),
                    time.start_ms.map_or_else(unknown, utc),
                    time.start_ms
                        .map_or_else(unknown, |start| format!("{} ms", time.end_ms - start)),
                    called_by(record.identity().parent.as_deref())
                )
            }
            // A run is in the store from its admission, and has its record from its
            // end: one with an admission and no record was admitted and has not ended.
            Some(Err(StoredRecordError::NoRecord)) | None => {
                match admission_of(&reader, &generation, id) {
                    // The core's sentence shows what it quotes escaped and cut already.
                    Some(Err(why)) => {
                        format!("{}\tno record; admission not read: {why}", shown(id))
                    }
                    Some(Ok(admission)) => format!(
                        "{}\tadmitted, not ended\t{} {}\t{}{}",
                        shown(id),
                        shown(admission.action()),
                        shown(admission.version()),
                        utc(admission.start_ms()),
                        called_by(admission.parent().map(|run| run.as_str()))
                    ),
                    None => format!("{}\tno record", shown(id)),
                }
            }
            // The core's sentence is escaped and cut where it quotes the store: it is
            // printed as it is.
            Some(Err(error)) => format!("{}\trecord not read: {error}", shown(id)),
        };
        text.push_str(&line);
        text.push('\n');
    }
    Ok(text)
}

/// What a line of `runs list` says after the status of a run whose record has lost
/// something (ADR 0013, "What is lost"): that it was interrupted, and how much is
/// lost. Nothing for a run that ended by itself. It goes by `outcome.lost`, as every
/// reader does, and never by the error's kind: an action may call its own error
/// `interrupted`, and nothing of that run is lost.
fn lost_in_a_line(record: &RunRecord) -> &'static str {
    let lost = &record.outcome().lost;
    if lost.is_empty() {
        ""
    } else if lost.contains(&Lost::Admission) {
        ", interrupted (everything is lost, its admission too)"
    } else {
        ", interrupted (everything but what its admission says is lost)"
    }
}

/// The run a caller means by `given`, for every command that takes a run: `runs show`
/// and `store delete` both ask here, so that the run that was looked at is the run
/// that is deleted.
///
/// A run's id always means that run. `list` prints an id with a control character
/// escaped, in a form no shell gives back, and cuts one over 64 characters, so a text
/// that is no run's id is taken as `list` prints a run, when it prints exactly one so.
/// Two cases are refused, and nothing is chosen: a text that is one run's id and also
/// how `list` prints another run, since whoever copied it from a line of `list` meant
/// the other; and a text that is no id and is how `list` prints several.
pub fn meant<'a>(generation: &'a Generation, given: &str) -> Result<(&'a str, &'a Run), String> {
    let runs = generation.runs();
    let listed_so = |(id, _): &(&String, &Run)| id.as_str() != given && shown(id) == given;
    let listed: Vec<_> = runs.iter().filter(listed_so).collect();
    match (runs.get_key_value(given), listed.as_slice()) {
        (Some((id, run)), []) => Ok((id.as_str(), run)),
        (None, [(id, run)]) => Ok((id.as_str(), run)),
        (None, []) => Err(format!("the store holds no run {}", shown(given))),
        // What `list` prints is printable as it is, and `given` is that, letter for
        // letter: it is said back unescaped, so that it reads as it was typed.
        (Some((id, _)), _) => Err(format!(
            "{given} is the id of one run and is how `runs list` prints another run. \
             `runs list` prints the first as {}: give that for it. Give the second by \
             its id, which holds a character that is printed escaped, or is longer than \
             is printed; `whizbang store list` prints ids whole, with their escapes",
            shown(id)
        )),
        (None, several) => Err(format!(
            "{} runs are listed as {}: an id over 64 characters is listed cut, and these \
             differ after that. Give the whole id, which `whizbang store list` prints",
            several.len(),
            shown(given)
        )),
    }
}

fn show(root: &Path, given: &str, json: bool, diagnostic: Option<&str>) -> Result<Vec<u8>, String> {
    let (reader, generation) = open(root)?;
    let (id, run) = meant(&generation, given)?;
    if let Some(name) = diagnostic {
        let Some(entry) = run.get(name) else {
            return Err(format!(
                "the run {} holds nothing named {}",
                shown(id),
                shown(name)
            ));
        };
        if entry.kind != Kind::Diagnostic {
            return Err(format!(
                "{} of the run {} is an output, not a diagnostic",
                shown(name),
                shown(id)
            ));
        }
        return reader.read(entry).map_err(|e| e.to_string());
    }
    let record = match record_of(&reader, &generation, id) {
        Some(Ok(record)) => record,
        Some(Err(StoredRecordError::NoRecord)) => {
            let admission = match admission_of(&reader, &generation, id) {
                Some(Ok(admission)) => admission,
                Some(Err(why)) => {
                    return Err(format!(
                        "the record of the run {} cannot be shown: the run has no record, \
                         and its admission is not read: {why}",
                        shown(id)
                    ));
                }
                None => {
                    return Err(format!(
                        "the record of the run {} cannot be shown: the run has no record",
                        shown(id)
                    ));
                }
            };
            if json {
                return Err(format!(
                    "the run {} has no record: it was admitted and has not ended; \
                     --diagnostic admitted prints its admission",
                    shown(id)
                ));
            }
            return Ok(admitted_text(&admission).into_bytes());
        }
        Some(Err(error)) => {
            return Err(format!(
                "the record of the run {} cannot be shown: {error}",
                shown(id)
            ));
        }
        None => return Err(format!("the store holds no run {}", shown(id))),
    };
    if json {
        return Ok(record.to_bytes());
    }
    let called = called_records(&reader, &generation, &record);
    Ok(text_of(&record, &called).into_bytes())
}

/// The most runs below one that `runs show` reads to add up what they cost.
const CALLED_LIMIT: usize = 512;

/// The records of the runs a run called, and the runs they called, breadth first, each
/// once, at most [`CALLED_LIMIT`]; a run whose record cannot be read is left out, and
/// [`text_of`] says how many were.
fn called_records(reader: &Reader, generation: &Generation, top: &RunRecord) -> Called {
    let mut called = Called::default();
    let mut waiting: std::collections::VecDeque<String> = top
        .steps()
        .iter()
        .map(|step| step.run().to_owned())
        .collect();
    // The top run is seen already: a step that names it again (a record that lists itself)
    // is not counted twice.
    let mut seen = std::collections::BTreeSet::from([top.identity().run.clone()]);
    while let Some(run) = waiting.pop_front() {
        if !seen.insert(run.clone()) {
            continue;
        }
        if called.records.len() >= CALLED_LIMIT {
            called.unread += 1;
            continue;
        }
        match record_of(reader, generation, &run) {
            Some(Ok(record)) => {
                waiting.extend(record.steps().iter().map(|step| step.run().to_owned()));
                called.records.push(record);
            }
            _ => called.unread += 1,
        }
    }
    called
}

/// The runs below one, as [`called_records`] reads them.
#[derive(Default)]
struct Called {
    records: Vec<RunRecord>,
    /// How many runs below it were named and could not be read.
    unread: usize,
}

/// The admission of a run that has something under `admitted`, or why it is not read,
/// as the core says it: escaped and cut where it quotes the store.
fn admission_of(
    reader: &Reader,
    generation: &Generation,
    id: &str,
) -> Option<Result<Admission, String>> {
    match admitted(generation, id, |entry| reader.read(entry))? {
        Ok(admission) => Some(Ok(admission)),
        Err(StoredAdmissionError::NoAdmission) => None,
        Err(error) => Some(Err(error.to_string())),
    }
}

/// The mark `runs list` gives a run that another run called: one more field, naming
/// that run. A run that nothing called has none.
fn called_by(parent: Option<&str>) -> String {
    match parent {
        Some(parent) => format!("\tcalled by {}", shown(parent)),
        None => String::new(),
    }
}

/// An amount of a record, in millionths of a US dollar, as money: as `whizbang spend
/// status` prints one.
fn usd(micros: u64) -> String {
    crate::spend::usd(u128::from(micros))
}

/// What was reserved for a run, for a person. "Nothing reserved" is a run that no
/// ledger reserved for; a run a ledger admitted at no cost reserved `$0.00`.
fn reserved(micros: Option<u64>) -> String {
    match micros {
        Some(micros) => format!("{} reserved", usd(micros)),
        None => "nothing reserved".to_owned(),
    }
}

/// What a run cost, for a person: what its own paid requests were settled at, added
/// up, and how much of that no answer settled, where it is not nothing. A request
/// that no answer settled is counted at its bound, so the sum is what the run can
/// have cost at the most: over, never under. What a run it called cost is in that
/// run's record.
fn cost(requests: &[Paid]) -> String {
    let sum = |of: fn(&Paid) -> u64| -> u128 {
        let amounts = requests.iter().map(|paid| u128::from(of(paid)));
        amounts.sum()
    };
    let settled = crate::spend::usd(sum(Paid::settled_micros));
    match sum(Paid::unresolved_micros) {
        0 => settled,
        unresolved => format!(
            "{settled}, of which {} is of requests that no answer settled",
            crate::spend::usd(unresolved)
        ),
    }
}

/// A run that was admitted and has not ended, for a person: what its admission holds.
/// A run's id and a request's id are held to their rules when an admission is read, and
/// an action's name and version to theirs; they are printed escaped all the same.
fn admitted_text(admission: &Admission) -> String {
    let request = match admission.request() {
        Some(request) => shown(request.as_str()),
        None => "none".to_owned(),
    };
    let parent = match admission.parent() {
        Some(parent) => shown(parent.as_str()),
        None => "none".to_owned(),
    };
    format!(
        "run: {}\nrequest: {request}\nparent: {parent}\naction: {} {}\nstatus: admitted, not ended\n\
         started: {}\nspend: {}\nrecord: none: the run was admitted and has not ended; it is executing, \
         or the process that ran it ended first\n",
        shown(admission.run().as_str()),
        shown(admission.action()),
        shown(admission.version()),
        utc(admission.start_ms()),
        reserved(admission.reserved_micros())
    )
}

/// A record for a person: every group, the empty ones said to be empty; and, for a run
/// that called others, what it and they cost together.
fn text_of(record: &RunRecord, called: &Called) -> String {
    let identity = record.identity();
    let asked = record.asked();
    let outcome = record.outcome();
    let time = record.time();
    // What the record does not know (ADR 0013): a group that `outcome.lost` names is
    // shown as unknown, and never as none or as nothing. An interrupted run reads as
    // one whose spend, requests and recipients are not known, not as one that had none.
    let lost = |group: Lost| outcome.lost.contains(&group);
    let unknown = || "unknown".to_owned();
    let counted = |group: Lost, items: usize| {
        if lost(group) { unknown() } else { count(items) }
    };
    let or_none = |value: &Option<String>| match value {
        Some(value) => shown(value),
        None if lost(Lost::Admission) => unknown(),
        None => "none".to_owned(),
    };
    let mut text = String::new();
    let mut line = |line: String| {
        text.push_str(&line);
        text.push('\n');
    };
    line(format!("run: {}", shown(&identity.run)));
    line(format!("request: {}", or_none(&identity.request)));
    line(format!("parent: {}", or_none(&identity.parent)));
    line(if lost(Lost::Version) {
        format!(
            "whizbang: {} (the version that ended it; the version that ran it is unknown)",
            shown(&identity.whizbang)
        )
    } else {
        format!("whizbang: {}", shown(&identity.whizbang))
    });
    line(format!(
        "action: {} {}",
        asked.action.as_deref().map_or_else(unknown, shown),
        asked.version.as_deref().map_or_else(unknown, shown)
    ));
    line(format!("status: {}", outcome.status.name()));
    if let Some(error) = &outcome.error {
        line(format!(
            "error: {}: {}",
            error.kind.escape_debug(),
            error.message.escape_debug()
        ));
    }
    if let Some(why) = &outcome.invalid {
        line(format!("invalid: {}", why.escape_debug()));
    }
    // What the run's action called and was refused: no run was made for these.
    if !outcome.lost.is_empty() {
        let names: Vec<String> = outcome
            .lost
            .iter()
            .map(|group| {
                let name = serde_json::to_value(group).unwrap_or_default();
                name.as_str().unwrap_or_default().to_owned()
            })
            .collect();
        line(format!("lost: {}", names.join(", ")));
    }
    line(format!(
        "refusals: {}",
        counted(Lost::Refusals, outcome.refusals.len())
    ));
    for refused in &outcome.refusals {
        let kind = serde_json::to_value(refused.kind()).unwrap_or_default();
        line(format!(
            "  {}\t{}\t{}",
            refused.action().map_or_else(|| "none".to_owned(), shown),
            kind.as_str().unwrap_or_default(),
            refused.why().escape_debug()
        ));
    }
    if outcome.refusals_omitted > 0 {
        line(format!(
            "  and {} more, not listed",
            outcome.refusals_omitted
        ));
    }
    line(format!(
        "started: {}",
        time.start_ms.map_or_else(unknown, utc)
    ));
    line(match time.start_ms {
        Some(start) => format!("ended: {} ({} ms)", utc(time.end_ms), time.end_ms - start),
        None => format!("ended: {}", utc(time.end_ms)),
    });
    line(format!(
        "config: {}",
        asked
            .config
            .map_or_else(unknown, |config| config.to_string())
    ));
    let valued = |value: &ArgumentValue| match value {
        ArgumentValue::Number(number) => format!("{number:?}"),
        ArgumentValue::Integer(number) => number.to_string(),
        ArgumentValue::Boolean(boolean) => boolean.to_string(),
        ArgumentValue::Text(text) => format!("\"{}\"", text.escape_debug()),
    };
    // The resolved settings, in the order of their names.
    line(format!(
        "settings: {}",
        counted(Lost::Settings, asked.settings.len())
    ));
    for setting in &asked.settings {
        line(format!(
            "  {} = {}",
            shown(setting.name()),
            valued(setting.value())
        ));
    }
    line(format!(
        "arguments: {}",
        counted(Lost::Arguments, asked.arguments.len())
    ));
    for argument in &asked.arguments {
        line(format!(
            "  {} = {}",
            shown(&argument.name),
            valued(&argument.value)
        ));
    }
    line(format!(
        "inputs: {}",
        counted(Lost::Inputs, record.inputs().len())
    ));
    for input in record.inputs() {
        line(format!(
            "  {}\t{} {}\tbinding {}",
            shown(&input.port),
            input.artifact_type,
            input.artifact,
            input.binding
        ));
    }
    line(format!("outputs: {}", count(record.outputs().len())));
    for output in record.outputs() {
        line(format!(
            "  {}\t{} {}",
            shown(&output.port),
            output.artifact_type,
            output.artifact
        ));
    }
    // Each capability request, in the order they were sent, with how long it took and
    // where it was answered (ADR 0025): the device, and whether a cache answered, each
    // `not known` where the answer did not say, as no answer of a `run/3` record did.
    // That is not `unknown`, which says a group of the record was lost.
    //
    // A model's names are printed whole, escaped: the reader holds each to 128 ASCII
    // characters, and a local model's version is its installation's identity, 71
    // characters, which a cut would leave unable to tell two installations apart.
    line(format!(
        "models: {}",
        counted(Lost::Models, record.models().len())
    ));
    for (at, model) in record.models().iter().enumerate() {
        let ended = serde_json::to_value(model.ended).unwrap_or_default();
        let took = record
            .time()
            .durations
            .get(at)
            .map_or(0, |duration| duration.ms);
        let device = model
            .device
            .as_deref()
            .map_or_else(|| "not known".to_owned(), whole);
        let cache = match model.cached {
            Some(true) => "cached",
            Some(false) => "not cached",
            None => "cache not known",
        };
        line(format!(
            "  {}\t{} {} {}\t{}\t{took} ms\tdevice {device}\t{cache}",
            model.capability.name(),
            whole(&model.backend),
            whole(&model.model),
            whole(&model.version),
            ended.as_str().unwrap_or_default()
        ));
    }
    // What was reserved, each paid request with what it was settled at, and what
    // they come to: what the run cost. With the admission lost, what was reserved is
    // not known either; with the requests lost, neither is the cost. A run that no
    // ledger reserved for and that lists no paid request, as every run of a build
    // before the ledger is, is said to have reserved nothing, and no cost is stated
    // of it: its requests were not counted. A run a ledger admitted at no cost
    // reserved `$0.00` and cost `$0.00`: it was counted, with nothing to spend.
    //
    // What was reserved is never `unknown` beside a count of requests. A record that
    // has lost its admission has lost every other group with it, its requests among
    // them (ADR 0013, "What is lost": what is lost is nothing, every group but the
    // admission, or every group, and the record's reader takes no other), so its
    // line is `spend: unknown; paid requests: unknown; cost: unknown`. The arm below
    // that states no cost is reached only by a record that has lost nothing.
    let spend = record.spend();
    let uncounted = spend.reserved_micros.is_none() && spend.requests.is_empty();
    let came_to = match (lost(Lost::Requests), uncounted) {
        (true, _) => format!("; cost: {}", unknown()),
        (false, false) => format!("; cost: {}", cost(&spend.requests)),
        (false, true) => String::new(),
    };
    line(format!(
        "spend: {}; paid requests: {}{came_to}",
        match spend.reserved_micros {
            None if lost(Lost::Admission) => unknown(),
            micros => reserved(micros),
        },
        counted(Lost::Requests, spend.requests.len())
    ));
    for paid in &spend.requests {
        line(format!(
            "  request {}\t{}\theld {}\tsettled {}\tunresolved {}{}",
            paid.request(),
            paid.stage().name(),
            usd(paid.reserved_micros()),
            usd(paid.settled_micros()),
            usd(paid.unresolved_micros()),
            if paid.is_over_bound() {
                "\tover its bound"
            } else {
                ""
            }
        ));
    }
    // What the run and every run below it cost together: a run that called others
    // (a recipe, a loop) sends no request of its own, and what its children were
    // settled at, a request no answer settled among them, is what the call cost.
    if !record.steps().is_empty() {
        let mut requests: Vec<Paid> = spend.requests.clone();
        for child in &called.records {
            requests.extend(child.spend().requests.iter().cloned());
        }
        let unread = match called.unread {
            0 => String::new(),
            1 => "; 1 run below it could not be read, so this is at least what it cost".to_owned(),
            n => format!("; {n} runs below it could not be read, so this is at least what it cost"),
        };
        let runs = match called.records.len() {
            1 => "1 run".to_owned(),
            n => format!("{n} runs"),
        };
        line(format!(
            "with the runs it called: {runs}; paid requests: {}; cost: {}{unread}",
            requests.len(),
            cost(&requests)
        ));
    }
    // Where the run's data went: an origin, the account cut, the classes, and
    // whether it was sent or refused.
    line(format!(
        "egress: {}",
        counted(Lost::Egress, record.egress().len())
    ));
    for went in record.egress() {
        let classes: Vec<&str> = went.classes().iter().map(|class| class.name()).collect();
        line(format!(
            "  {}\t{}\t{}\t{}",
            shown(went.origin()),
            went.account().map_or_else(|| "none".to_owned(), shown),
            classes.join(", "),
            match went.outcome() {
                Handed::Sent => "sent",
                Handed::Refused => "refused",
            }
        ));
    }
    // The runs this one called, in order: each is shown by its own id.
    line(format!(
        "steps: {}",
        counted(Lost::Steps, record.steps().len())
    ));
    for step in record.steps() {
        line(format!(
            "  call\t{}\t{}\t{}",
            shown(step.run()),
            shown(step.action()),
            step.status().map_or("not ended", |status| status.name())
        ));
    }
    // A loop's attempts (plan m4, task L1): each with its verdict or the reason it has
    // none, what its retry changed, how many runs it made and what its requests
    // settled; then which was kept, and why the attempts ended. Only a loop has them.
    if let Some(attempts) = record.attempts() {
        line(format!("attempts: {}", attempts.list.len()));
        for attempt in &attempts.list {
            let word = |value: serde_json::Value| value.as_str().unwrap_or("none").to_owned();
            let verdict = match (&attempt.verdict, &attempt.unchecked) {
                (Some(decision), _) => word(serde_json::to_value(decision).unwrap_or_default()),
                (None, Some(why)) => format!("not checked ({})", shown(why)),
                (None, None) => "not checked".to_owned(),
            };
            let changed = match &attempt.changed {
                None => "nothing changed".to_owned(),
                Some(record::Changed::Wider { px }) => format!("a wider mask, grow px {px}"),
                Some(record::Changed::Located { crops }) => {
                    format!("{} crops looked into", crops.len())
                }
            };
            line(format!(
                "  attempt {}\t{verdict}\t{changed}\t{} runs\tsettled {}",
                attempt.attempt,
                attempt.runs.len(),
                crate::spend::usd(u128::from(attempt.settled_micros))
            ));
        }
        line(format!(
            "  kept: {}",
            attempts
                .kept
                .map_or_else(|| "none".to_owned(), |kept| format!("attempt {kept}"))
        ));
        line(format!("  ended: {}", shown(&attempts.ended)));
    }
    // The judge's turns, and the verdict when the run concluded one. The whole of
    // each exchange is the diagnostic the turn names.
    line(format!(
        "judge: {}",
        counted(Lost::Judge, record.judge().len())
    ));
    for entry in record.judge() {
        let word = |value: serde_json::Value| value.as_str().unwrap_or("none").to_owned();
        match entry {
            Judged::Turn {
                exchange,
                stop,
                tool_calls,
                excluded,
                ..
            } => line(format!(
                "  turn\t{}\t{} tool calls\tleft out: {}\t{}",
                word(serde_json::to_value(stop).unwrap_or_default()),
                tool_calls,
                word(serde_json::to_value(excluded).unwrap_or_default()),
                exchange
                    .as_deref()
                    .map_or_else(|| "not kept".to_owned(), shown)
            )),
            Judged::Verdict { decision, why } => line(format!(
                "  verdict\t{}\t{}",
                word(serde_json::to_value(decision).unwrap_or_default()),
                why.escape_debug()
            )),
        }
    }
    line(format!(
        "diagnostics: {}",
        counted(Lost::Diagnostics, record.diagnostics().len())
    ));
    for diagnostic in record.diagnostics() {
        line(format!(
            "  {}\t{} bytes\t{}",
            shown(diagnostic.name()),
            diagnostic.bytes(),
            diagnostic.digest()
        ));
    }
    text
}

fn count(count: usize) -> String {
    if count == 0 {
        "none".to_owned()
    } else {
        count.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::utc;

    #[test]
    fn a_time_is_printed_as_its_date_in_utc() {
        assert_eq!(utc(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(utc(951_782_400_000), "2000-02-29T00:00:00.000Z");
        assert_eq!(utc(1_794_519_513_123), "2026-11-12T21:38:33.123Z");
        assert_eq!(utc(4_102_444_799_999), "2099-12-31T23:59:59.999Z");
        assert_eq!(utc(1_709_251_200_000), "2024-03-01T00:00:00.000Z");
    }
}
