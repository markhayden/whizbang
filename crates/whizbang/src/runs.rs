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
use whizbang_core::contract::record::{self, ArgumentValue, Judged, RunRecord, StoredRecordError};
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
    }
}

/// How much of a text that was read from a store, or given on the command line, a
/// message shows: its first 64 characters, escaped.
fn shown(text: &str) -> String {
    let cut: String = text.chars().take(64).collect();
    let more = if text.chars().count() > 64 { "..." } else { "" };
    format!("{}{more}", cut.escape_debug())
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
                    "{}\t{}\t{} {}\t{}\t{}{}",
                    shown(id),
                    record.outcome().status.name(),
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
    Ok(text_of(&record).into_bytes())
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

/// What was reserved for a run, for a person.
fn reserved(micros: Option<u64>) -> String {
    match micros {
        Some(micros) => format!("{micros} millionths of a US dollar reserved"),
        None => "nothing reserved".to_owned(),
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

/// A record for a person: every group, the empty ones said to be empty.
fn text_of(record: &RunRecord) -> String {
    let identity = record.identity();
    let asked = record.asked();
    let outcome = record.outcome();
    let time = record.time();
    let or_none = |value: &Option<String>| match value {
        Some(value) => shown(value),
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
    line(format!("whizbang: {}", shown(&identity.whizbang)));
    // What the record of a run a dead process left does not know is shown as
    // unknown (`outcome.lost` says which groups those are).
    let unknown = || "unknown".to_owned();
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
    line(format!("refusals: {}", count(outcome.refusals.len())));
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
    line("settings: none".to_owned());
    line(format!("arguments: {}", count(asked.arguments.len())));
    for argument in &asked.arguments {
        let value = match &argument.value {
            ArgumentValue::Number(number) => format!("{number:?}"),
            ArgumentValue::Integer(number) => number.to_string(),
            ArgumentValue::Boolean(boolean) => boolean.to_string(),
            ArgumentValue::Text(text) => format!("\"{}\"", text.escape_debug()),
        };
        line(format!("  {} = {value}", shown(&argument.name)));
    }
    line(format!("inputs: {}", count(record.inputs().len())));
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
    // Each capability request, in the order they were sent, with how long it took.
    line(format!("models: {}", count(record.models().len())));
    for (at, model) in record.models().iter().enumerate() {
        let ended = serde_json::to_value(model.ended).unwrap_or_default();
        let took = record
            .time()
            .durations
            .get(at)
            .map_or(0, |duration| duration.ms);
        line(format!(
            "  {}\t{} {} {}\t{}\t{took} ms",
            model.capability.name(),
            shown(&model.backend),
            shown(&model.model),
            shown(&model.version),
            ended.as_str().unwrap_or_default()
        ));
    }
    line(format!(
        "spend: {}; paid requests: none",
        reserved(record.spend().reserved_micros)
    ));
    line("egress: none".to_owned());
    // The runs this one called, in order: each is shown by its own id.
    line(format!("steps: {}", count(record.steps().len())));
    for step in record.steps() {
        line(format!(
            "  call\t{}\t{}\t{}",
            shown(step.run()),
            shown(step.action()),
            step.status().map_or("not ended", |status| status.name())
        ));
    }
    // The judge's turns, and the verdict when the run concluded one. The whole of
    // each exchange is the diagnostic the turn names.
    line(format!("judge: {}", count(record.judge().len())));
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
        count(record.diagnostics().len())
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
