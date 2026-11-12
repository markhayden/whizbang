//! `whizbang runs list|show`: finding and reading runs (`SPEC.md` section 3, "Metrics
//! from ordinary runs"; ADR 0005). Both only read, so both work while another process
//! writes the store.
//!
//! What a store holds is not trusted to be what this build would have written. A run's
//! id is whatever the store has it under, and is taken as given: `show` looks its
//! argument up as it is, and does not hold it to the rule for the ids the executor
//! makes, or a run that `list` shows could not be shown. Everything read from the store
//! is printed escaped, so nothing in it reaches a terminal as a control character,
//! except where the caller asks for bytes as they are stored (`--json`, `--diagnostic`).

use std::path::Path;
use std::sync::Arc;

use clap::Subcommand;
use whizbang_core::contract::record::{self, ArgumentValue, RunRecord, StoredRecordError};
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Generation, Kind, Reader};

use crate::store::Where;

#[derive(Subcommand)]
pub enum Command {
    /// List the runs the store holds, each with how it ended, its action, when it
    /// started and how long it took. A run that has no record, or one this build cannot
    /// read, is listed with the reason.
    List {
        #[command(flatten)]
        at: Where,
    },
    /// Show one run's record: what was asked, how it ended, and what it kept.
    Show {
        /// The run, by its id, as `runs list` shows it.
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
            Some(Ok(record)) => format!(
                "{}\t{}\t{} {}\t{}\t{} ms",
                shown(id),
                record.outcome().status.name(),
                shown(&record.asked().action),
                shown(&record.asked().version),
                utc(record.time().start_ms),
                record.time().end_ms - record.time().start_ms
            ),
            Some(Err(StoredRecordError::NoRecord)) | None => {
                format!("{}\tno record", shown(id))
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

fn show(root: &Path, id: &str, json: bool, diagnostic: Option<&str>) -> Result<Vec<u8>, String> {
    let (reader, generation) = open(root)?;
    let Some(run) = generation.run(id) else {
        return Err(format!("the store holds no run {}", shown(id)));
    };
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
    line(format!(
        "action: {} {}",
        shown(&asked.action),
        shown(&asked.version)
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
    line(format!("started: {}", utc(time.start_ms)));
    line(format!(
        "ended: {} ({} ms)",
        utc(time.end_ms),
        time.end_ms - time.start_ms
    ));
    line(format!("config: {}", asked.config));
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
    for group in ["models", "spend", "egress", "steps", "judge"] {
        line(format!("{group}: none"));
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
