//! What the command line does around the executor: the first door (`SPEC.md` section 3,
//! "The execution contract"). A door turns what a caller typed into a request, hands it
//! to the executor's one entry point, and says what came back.
//!
//! Three rules hold for everything here, and for each door after this one:
//!
//! - **A request id goes through the executor** ([`Executor::request_id`]), which looks
//!   for a secret in the text before anything shows it, and never through
//!   [`whizbang_core::contract::RequestId::parse`], whose refusal shows the text.
//! - **Nothing that was given is printed as it was.** What the executor says of a run
//!   has had the secrets taken out and been cut to a length already; the door then
//!   escapes it ([`printable`]), so that no control character reaches a terminal. And
//!   the binary passes every complaint of every command through the same escape where
//!   it prints it, a store's failure that names a path among them. What a caller typed
//!   and the executor never saw, such as text that is no artifact identity, is not said
//!   back at all: the door has no list of secrets to take out of it. One thing does
//!   say back what was typed, the parser of the command line in its usage errors: the
//!   binary prints those through the same escape, line by line.
//! - **A panic prints one fixed line** ([`quiet_panics`]). The default hook prints the
//!   panic's whole text and its place in the source, with no secret taken out.
//!
//! One process owns every run it makes: the command runs its action in its own process
//! and waits for it, so the run's cancellation, status and wait are that process's
//! (ADR 0008, "Cancellation"). A second process sees such a run as admitted and not
//! ended, and cannot cancel it; a process that is killed while its action runs leaves
//! the run so (`docs/carry-over.md`).

use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use whizbang_core::artifact::ArtifactId;
use whizbang_core::contract::record::DIAGNOSTICS_PREFIX;
use whizbang_core::contract::{
    Answer, Call, CallError, Committed, Executor, Outcome, Request, RunId, RunState, Value,
};
use whizbang_core::identity::Digest;
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Kind, Reader};
use whizbang_rascal::{grow, segment};

/// What a panic prints, after `whizbang: `: this line and nothing else.
pub const PANIC_LINE: &str = "an internal error (a panic) happened; what it said is not printed, because it may hold a secret";

/// The most characters of one text a door prints. What the executor says is cut far
/// under this already; it is the floor under that.
const PRINT_LIMIT: usize = 5000;

/// Replaces the process's panic hook with one that prints [`PANIC_LINE`] and nothing of
/// the panic: not its text, which can quote anything an action was given, and not its
/// place in the source. A panic inside an action is caught by the executor and ends
/// its run as an error, with the secrets taken out of what it records (ADR 0008, "A
/// panic in an action"); a panic anywhere else ends the process, with exit code 101.
pub fn quiet_panics() {
    std::panic::set_hook(Box::new(|_| {
        // A failure to write this line is nothing a hook can report.
        let _ = writeln!(std::io::stderr(), "whizbang: {PANIC_LINE}");
    }));
}

/// A text as a door prints it: every character that is not printable escaped. Quotes
/// and the backslash are left as they are, so a text that was escaped before, as the
/// executor's refusals are where they quote something, is not escaped a second time,
/// and escaping twice is escaping once. The price: a backslash an action wrote cannot
/// be told from one an escape made. A text over [`PRINT_LIMIT`] characters is cut there, and three dots
/// follow. The text is redacted before it comes here: what is cut or escaped first can
/// hide a secret from the pass that takes it out.
pub fn printable(text: &str) -> String {
    let mut out = String::new();
    for (count, c) in text.chars().enumerate() {
        if count == PRINT_LIMIT {
            out.push_str("...");
            break;
        }
        match c {
            '\'' | '"' | '\\' => out.push(c),
            other => out.extend(other.escape_debug()),
        }
    }
    out
}

/// What `whizbang rascal grow` was asked, as it was typed.
#[derive(Debug, Clone, Copy)]
pub struct GrowAsked<'a> {
    /// The mask, by its identity: `sha256:` and 64 hex digits.
    pub mask: &'a str,
    pub px: f64,
    pub down_px: Option<f64>,
    /// The caller's id for the request, when it gave one.
    pub request_id: Option<&'a str>,
}

/// A refusal, or a failure of the store, in the executor's sentence as it is: what it
/// quotes it has escaped and cut already, and what it says of the store holds no path.
/// The binary escapes every complaint once more where it prints it ([`printable`]).
fn refused(error: &CallError) -> String {
    error.to_string()
}

/// Runs `grow` through the executor, and waits for it. What comes back is what to
/// print when the run completed, or the complaint: both are escaped already.
pub fn grow(executor: &dyn Executor, asked: &GrowAsked<'_>) -> Result<String, String> {
    let Ok(mask) = asked.mask.parse::<Digest>() else {
        return Err(
            "--mask is no artifact identity: an identity is sha256: and 64 lower-case hex \
             digits, as `whizbang store import` prints it"
                .to_owned(),
        );
    };
    let mut call = Call::new(grow::NAME)
        .argument("px", Value::Number(asked.px))
        .input("mask", ArtifactId(mask));
    if let Some(down_px) = asked.down_px {
        call = call.argument("down_px", Value::Number(down_px));
    }
    let (_, mut text, outputs) = completed(executor, call, asked.request_id)?;
    for output in outputs {
        let port = printable(output.port());
        text.push_str(&format!("{port} {}\n", output.id().0));
    }
    Ok(text)
}

/// What `whizbang rascal segment` was asked, as it was typed.
#[derive(Debug, Clone, Copy)]
pub struct SegmentAsked<'a> {
    /// The image, by its identity: `sha256:` and 64 hex digits.
    pub image: &'a str,
    pub phrase: &'a str,
    pub max_instances: u32,
    /// The segmenter, by its name in the config, when the caller chose one.
    pub backend: Option<&'a str>,
    /// The caller's id for the request, when it gave one.
    pub request_id: Option<&'a str>,
}

/// What reads the bytes a run kept under a diagnostic name, the run's id given.
pub type ReadDiagnostic<'a> = dyn Fn(&RunId, &str) -> Result<Vec<u8>, String> + 'a;

/// Runs `segment` through the executor, and waits for it. What comes back is what to
/// print when the run completed, or the complaint: both are escaped already. A run
/// that completed prints its line, then each mask's identity with the score and the
/// box the run's diagnostic gives it (`whizbang_rascal::segment::DIAGNOSTIC`), which
/// `read` hands over: the bytes the run kept under that name, the run's id given.
/// A run that found nothing says so.
pub fn segment(
    executor: &dyn Executor,
    asked: &SegmentAsked<'_>,
    read: &ReadDiagnostic<'_>,
) -> Result<String, String> {
    let Ok(image) = asked.image.parse::<Digest>() else {
        return Err(
            "--image is no artifact identity: an identity is sha256: and 64 lower-case hex \
             digits, as `whizbang store import` prints it"
                .to_owned(),
        );
    };
    let mut call = Call::new(segment::NAME)
        .argument("phrase", Value::text(asked.phrase))
        .argument(
            "max_instances",
            Value::Integer(i64::from(asked.max_instances)),
        )
        .input("image", ArtifactId(image));
    if let Some(backend) = asked.backend {
        call = call.argument("backend", Value::text(backend));
    }
    let (run, mut text, outputs) = completed(executor, call, asked.request_id)?;
    if outputs.is_empty() {
        text.push_str("no instance was found\n");
        return Ok(text);
    }
    // The diagnostic says each mask with its score and its box, one line each. A run
    // whose diagnostic cannot be read still printed its masks.
    let said = read(&run, segment::DIAGNOSTIC).ok();
    let said = said.and_then(|bytes| String::from_utf8(bytes).ok());
    let said = said.unwrap_or_default();
    for output in outputs {
        let id = output.id().0.to_string();
        let beside = said
            .lines()
            .find_map(|line| line.strip_prefix(&format!("mask {id} ")))
            .map_or_else(|| "score unknown".to_owned(), printable);
        text.push_str(&format!("{} {id} {beside}\n", printable(output.port())));
    }
    Ok(text)
}

/// The bytes a run kept under this diagnostic name, read from the store at `root` by
/// a reader of its own; or why they were not, in the store's words.
pub fn diagnostic(root: &Path, run: &RunId, name: &str) -> Result<Vec<u8>, String> {
    let reader = Reader::open(root, Arc::new(RealFs)).map_err(|e| e.to_string())?;
    let generation = reader.snapshot().map_err(|e| e.to_string())?;
    let held = generation.runs().get(run.as_str());
    let entry = held.and_then(|held| held.get(&format!("{DIAGNOSTICS_PREFIX}{name}")));
    match entry {
        Some(entry) if entry.kind == Kind::Diagnostic => {
            reader.read(entry).map_err(|e| e.to_string())
        }
        _ => Err("the run keeps no diagnostic of that name".to_owned()),
    }
}

/// Sends the call, under the caller's request id when it gave one, and waits for its
/// run. A run that completed is its id, its line to print and its outputs; any other
/// end is the complaint, escaped already.
fn completed(
    executor: &dyn Executor,
    call: Call,
    request_id: Option<&str>,
) -> Result<(RunId, String, Vec<Committed>), String> {
    let mut request = Request::new(call);
    if let Some(text) = request_id {
        request = request.with_id(executor.request_id(text).map_err(|e| refused(&e))?);
    }
    let (id, state, before) = match executor.request(&request).map_err(|e| refused(&e))? {
        Answer::Ran(finished) => (
            finished.run().clone(),
            RunState::Ended(finished.outcome().clone()),
            "",
        ),
        Answer::Repeat(earlier) => (
            earlier.run().clone(),
            earlier.state().clone(),
            " (the request was sent before, and nothing ran again)",
        ),
        // `request` waits; only `start` answers so.
        Answer::Started(run) => (run, RunState::NotEnded { here: true }, ""),
    };
    let run = format!("run {}{before}", printable(id.as_str()));
    let nothing = "nothing was committed";
    match state {
        RunState::Ended(Outcome::Completed { outputs }) => {
            Ok((id, format!("{run} completed\n"), outputs))
        }
        RunState::Ended(Outcome::Cancelled) => Err(format!("{run} was cancelled; {nothing}")),
        RunState::Ended(Outcome::Error { kind, message }) => Err(format!(
            "{run} ended in an error ({}): {}; {nothing}",
            printable(&kind),
            printable(&message)
        )),
        RunState::Ended(Outcome::Invalid { why }) => Err(format!(
            "{run} found what it was given invalid: {}; {nothing}",
            printable(&why)
        )),
        RunState::NotEnded { .. } => Err(format!("{run} was admitted and has not ended")),
        RunState::Abandoned { failure } => Err(format!(
            "{run} was admitted and will not end here{}",
            failure.map_or(String::new(), |failure| format!(
                ": the store failed: {}",
                printable(&failure.to_string())
            ))
        )),
        RunState::Removed { ended } => Err(format!(
            "{run} is no longer in the store: it was deleted{}",
            ended.map_or(String::new(), |status| format!(
                " after it ended as {}",
                status.name()
            ))
        )),
    }
}
