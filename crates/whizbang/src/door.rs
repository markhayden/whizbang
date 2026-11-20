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

use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;

use whizbang_core::artifact::ArtifactId;
use whizbang_core::contract::record::DIAGNOSTICS_PREFIX;
use whizbang_core::contract::{
    Answer, Call, CallError, Committed, Executor, Outcome, Recovery, Request, RunId, RunState,
    Value,
};
use whizbang_core::identity::Digest;
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Kind, Reader};
use whizbang_rascal::{grow, segment};
use whizbang_squint::job::Job;

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

/// What `whizbang rascal select`, `plug`, `feather` or `render` was asked: the action,
/// its arguments, and its inputs by port as they were typed, an identity each.
#[derive(Debug, Clone)]
pub struct OperationAsked<'a> {
    pub action: &'a str,
    pub arguments: Vec<(&'a str, Value)>,
    /// Each input's port, the flag it was typed under, and the text typed.
    pub inputs: Vec<(&'a str, &'a str, &'a str)>,
    /// The caller's id for the request, when it gave one.
    pub request_id: Option<&'a str>,
}

/// Runs one of rascal's operations through the executor, and waits for it, as
/// [`grow`] does: what comes back is what to print when the run completed, each output
/// by its port and identity, or the complaint, escaped already.
pub fn operation(executor: &dyn Executor, asked: &OperationAsked<'_>) -> Result<String, String> {
    operation_saying(executor, asked, &mut on_the_error_stream)
}

/// What a door is told while a call is on its way: the plan of a planned call, before
/// the call runs.
pub type Say<'a> = &'a mut dyn FnMut(&str);

/// Says a line on the error stream, as the binary does while a call is on its way.
pub fn on_the_error_stream(line: &str) {
    eprintln!("{}", printable(line));
}

/// [`operation`], telling `say` the plan of a planned call before the call runs (ADR
/// 0004, "A call's plan").
pub fn operation_saying(
    executor: &dyn Executor,
    asked: &OperationAsked<'_>,
    say: Say<'_>,
) -> Result<String, String> {
    let mut call = Call::new(asked.action);
    for (name, value) in &asked.arguments {
        call = call.argument(name, value.clone());
    }
    for (port, flag, typed) in &asked.inputs {
        let Ok(id) = typed.parse::<Digest>() else {
            return Err(format!(
                "{flag} is no artifact identity: an identity is sha256: and 64 lower-case hex \
                 digits, as `whizbang store import` prints it"
            ));
        };
        call = call.input(port, ArtifactId(id));
    }
    let (_, mut text, outputs) = completed_saying(executor, call, asked.request_id, say)?;
    for output in outputs {
        let port = printable(output.port());
        text.push_str(&format!("{port} {}\n", output.id().0));
    }
    Ok(text)
}

/// Runs a recipe through the executor, and waits for it (ADR 0033). What comes back is
/// what to print when the run completed: its line, each step of its record as `step <id>
/// <action> run <run>`, and then its outputs, or, for a run that stopped after a step
/// (`until`), that step's outputs, read from that step's run: each as `<port> <identity>`.
/// Or the complaint, escaped already. `root` is the store's, which a reader of its own
/// reads the records from.
pub fn recipe(
    executor: &dyn Executor,
    asked: &OperationAsked<'_>,
    root: &Path,
) -> Result<String, String> {
    let mut call = Call::new(asked.action);
    for (name, value) in &asked.arguments {
        call = call.argument(name, value.clone());
    }
    for (port, flag, typed) in &asked.inputs {
        let Ok(id) = typed.parse::<Digest>() else {
            return Err(format!(
                "{flag} is no artifact identity: an identity is sha256: and 64 lower-case hex \
                 digits, as `whizbang store import` prints it"
            ));
        };
        call = call.input(port, ArtifactId(id));
    }
    let (run, mut text, mut outputs) = completed(executor, call, asked.request_id)?;
    let read = |run: &str| -> Result<whizbang_core::contract::RunRecord, String> {
        let reader = Reader::open(root, Arc::new(RealFs)).map_err(|e| e.to_string())?;
        let generation = reader.snapshot().map_err(|e| e.to_string())?;
        let stored =
            whizbang_core::contract::record::stored(&generation, run, |entry| reader.read(entry));
        match stored {
            Some(Ok(record)) => Ok(record),
            Some(Err(error)) => Err(format!(
                "the record of the run {} cannot be read: {}",
                printable(run),
                printable(&error.to_string())
            )),
            None => Err(format!("the run {} is not in the store", printable(run))),
        }
    };
    let record = read(run.as_str())?;
    let mut last = None;
    for step in record.steps() {
        let id = step.recipe().map_or("-", |recipe| recipe.id.as_str());
        text.push_str(&format!(
            "step {} {} run {}\n",
            printable(id),
            printable(step.action()),
            printable(step.run())
        ));
        last = Some(step.run().to_owned());
    }
    let until = asked
        .arguments
        .iter()
        .any(|(name, _)| *name == whizbang_core::recipe::UNTIL);
    if until && let Some(last) = last {
        let step = read(&last)?;
        outputs = (step.outputs().iter())
            .map(|output| {
                Committed::new(
                    &output.port,
                    output.artifact_type,
                    ArtifactId(output.artifact),
                )
            })
            .collect();
    }
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
    /// What the query is: `phrase`, `point` or `box`.
    pub kind: &'a str,
    /// The phrase in words, the point as `X Y`, or the box as `L T R B`.
    pub query: &'a str,
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
        .argument("kind", Value::text(asked.kind))
        .argument("query", Value::text(asked.query))
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

/// The masks a `segment` run committed, in the order it printed them, from the store at
/// `root` by a reader of its own: what `whizbang rascal select --run` keeps from. A run
/// that is not in the store, is not a run of `segment`, or did not complete is refused;
/// nothing typed is said back.
pub fn run_masks(root: &Path, run: &str) -> Result<Vec<String>, String> {
    let reader = Reader::open(root, Arc::new(RealFs)).map_err(|e| e.to_string())?;
    let generation = reader.snapshot().map_err(|e| e.to_string())?;
    let stored =
        whizbang_core::contract::record::stored(&generation, run, |entry| reader.read(entry));
    let record = match stored {
        None => return Err("--run names no run in the store".to_owned()),
        Some(Err(error)) => {
            return Err(format!(
                "--run names a run whose record cannot be read: {}",
                printable(&error.to_string())
            ));
        }
        Some(Ok(record)) => record,
    };
    if record.asked().action.as_deref() != Some(segment::NAME) {
        return Err(
            "--run names a run of another action: select keeps masks of a segment run".to_owned(),
        );
    }
    let masks: Vec<String> = record
        .outputs()
        .iter()
        .filter(|output| output.port == "mask")
        .map(|output| output.artifact.to_string())
        .collect();
    if masks.is_empty() {
        return Err("--run names a segment run that committed no mask".to_owned());
    }
    Ok(masks)
}

/// The masks `--pick` keeps of a run's: the Nth for each N, counted from 1, in the order
/// the picks were given. A pick past the run's masks is refused, naming how many it has.
pub fn picked(masks: &[String], picks: &[u32]) -> Result<Vec<String>, String> {
    picks
        .iter()
        .map(|pick| {
            let at = usize::try_from(*pick).ok().and_then(|n| n.checked_sub(1));
            at.and_then(|at| masks.get(at)).cloned().ok_or_else(|| {
                format!(
                    "--pick {pick} is past the run's masks: it has {} (from 1 to {})",
                    masks.len(),
                    masks.len()
                )
            })
        })
        .collect()
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
    completed_saying(executor, call, request_id, &mut on_the_error_stream)
}

/// [`completed`], telling `say` the plan of a planned call before the call runs: the
/// call is prepared first, and when its action planned it the plan is said in one line
/// and the request comes with that preparation, so that what runs is the plan that
/// was said, or nothing (a preparation whose plan would differ at admission is
/// refused as stale). A call that plans nothing is sent as it always was. A request
/// that was sent before is answered with its first run, whose own plan is in that
/// run's record: the line said here is of the call as it prepares now.
fn completed_saying(
    executor: &dyn Executor,
    call: Call,
    request_id: Option<&str>,
    say: Say<'_>,
) -> Result<(RunId, String, Vec<Committed>), String> {
    // A call that does not prepare is judged by the request, which also answers a
    // request that was sent before, whatever its call would be now.
    let planned = executor.prepare(&call).ok().filter(|p| p.plan().is_some());
    let mut request = Request::new(call);
    if let Some(prepared) = &planned {
        say(&prepared.plan().map(|plan| plan.line()).unwrap_or_default());
        request = request.expecting(prepared);
    }
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
    match state {
        RunState::Ended(outcome) => {
            let (text, outputs) = ended(&run, outcome)?;
            Ok((id, text, outputs))
        }
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

/// What a door says of a run that ended, after `run`, the run's words: the line of a
/// run that completed, and its outputs; or the complaint, escaped already.
pub fn ended(run: &str, outcome: Outcome) -> Result<(String, Vec<Committed>), String> {
    let nothing = "nothing was committed";
    match outcome {
        Outcome::Completed { outputs } => Ok((format!("{run} completed\n"), outputs)),
        Outcome::Cancelled => Err(format!("{run} was cancelled; {nothing}")),
        Outcome::Error { kind, message } => Err(format!(
            "{run} ended in an error ({}): {}; {nothing}",
            printable(&kind),
            printable(&message)
        )),
        Outcome::Invalid { why } => Err(format!(
            "{run} found what it was given invalid: {}; {nothing}",
            printable(&why)
        )),
    }
}

/// What `whizbang squint check` was asked, as it was typed.
#[derive(Debug, Clone, Copy)]
pub struct CheckAsked<'a> {
    /// The use case, one of the table's.
    pub use_case: &'a str,
    /// The edit, or the picture, that is judged, by its identity.
    pub candidate: &'a str,
    /// What it was edited from, by its identity, when one is given.
    pub source: Option<&'a str>,
    /// What it is held to, by their identities, in the order given.
    pub references: &'a [&'a str],
    /// The judge, by its name in the config's `[backends.judge]` tables, when one is
    /// named.
    pub judge: Option<&'a str>,
    /// The job's requirements in words, as data for the judge, when they are given.
    pub brief: Option<&'a str>,
    /// The caller's id for the request, when it gave one.
    pub request_id: Option<&'a str>,
    /// What is measured before the judge's first turn: `first` (the command's default)
    /// or `none` (ADR 0029).
    pub details: Details,
    /// The tools whose details are measured first, by name, when the caller names them.
    pub detail: Option<&'a str>,
    /// How a call of `align` by the judge is answered: `full` or `visual_only`.
    pub projection: &'a str,
}

/// What `squint check` measures before the judge's first turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Details {
    /// The details of each pair the use case relates, in the opening message; the judge
    /// may call `view`, `align` and `resize`.
    First,
    /// Nothing: the judge is told of `view` and `align` and calls them, as squint's arm C.
    None,
}

impl Details {
    /// The tools the judge is offered, and the name `check` takes.
    fn tools_and_name(self) -> (&'static str, &'static str) {
        match self {
            Details::First => ("view,align,resize", "first"),
            Details::None => ("view,align", "none"),
        }
    }
}

/// The most references `squint check` is given (plan 2C, task A1, item 2): with a
/// candidate and a source, the most images `check` takes.
pub const REFERENCES_LIMIT: usize = 8;

/// The most bytes of a job file `squint check --job` reads.
pub const JOB_FILE_LIMIT: u64 = 1024 * 1024;

fn identity(flag: &str, text: &str) -> Result<ArtifactId, String> {
    text.parse::<Digest>().map(ArtifactId).map_err(|_| {
        format!(
            "--{flag} is no artifact identity: an identity is sha256: and 64 lower-case hex \
             digits, as `whizbang store import` prints it"
        )
    })
}

/// The job the command line states (`Job::shorthand`), and its images' identities in the
/// job's order; or why the command is refused before anything runs: more references
/// than [`REFERENCES_LIMIT`], `brief-compliance` with no brief, or a text that is no
/// identity.
pub fn shorthand(asked: &CheckAsked<'_>) -> Result<(Job, Vec<ArtifactId>), String> {
    let given = asked.references.len();
    if given > REFERENCES_LIMIT {
        return Err(format!(
            "--reference is given at most {REFERENCES_LIMIT} times, and it was given {given}"
        ));
    }
    let briefed = asked.brief.is_some_and(|brief| !brief.trim().is_empty());
    if asked.use_case == "brief-compliance" && !briefed {
        return Err(
            "brief-compliance holds the picture to a brief: give it with --brief".to_owned(),
        );
    }
    let mut images = vec![identity("candidate", asked.candidate)?];
    if let Some(source) = asked.source {
        images.push(identity("source", source)?);
    }
    for reference in asked.references {
        images.push(identity("reference", reference)?);
    }
    let job = Job::shorthand(asked.use_case, asked.source.is_some(), given, asked.brief);
    Ok((job, images))
}

/// A squint job file, read whole (`Job::from_toml`) and of the use case typed, with each
/// of its pictures brought into the store at `root` from the file its `source.path`
/// names, relative to the job file's folder, and refused when the job pins its SHA-256
/// and the file's is another: the job and its images' identities, in its order. A
/// picture given inline is not taken. Nothing is sent before every picture is in.
pub fn job_file(
    file: &Path,
    use_case: &str,
    root: &Path,
    recovery: &Recovery,
) -> Result<(Job, Vec<ArtifactId>), String> {
    let shown = printable(&file.display().to_string());
    let mut text = String::new();
    let opened = std::fs::File::open(file).map_err(|e| format!("{shown}: {e}"))?;
    let read = opened.take(JOB_FILE_LIMIT + 1).read_to_string(&mut text);
    read.map_err(|e| format!("{shown}: {e}"))?;
    if text.len() as u64 > JOB_FILE_LIMIT {
        return Err(format!(
            "{shown}: a job file is at most {JOB_FILE_LIMIT} bytes"
        ));
    }
    let job = Job::from_toml(&text).map_err(|why| format!("{shown}: {}", printable(&why)))?;
    if job.use_case != use_case {
        return Err(format!(
            "{shown} is a job of another use case than {use_case}"
        ));
    }
    let folder = file.parent().unwrap_or(Path::new("."));
    let mut images = Vec::new();
    for image in &job.images {
        let source = image.source.as_ref();
        let Some(path) = source.and_then(|source| source.path.as_deref()) else {
            return Err(format!(
                "{shown}: image {} names no file: a picture given inline is not taken",
                printable(&image.id)
            ));
        };
        let sha256 = source.and_then(|source| source.sha256.as_deref());
        let picture = folder.join(path);
        images.push(crate::import::job_picture(
            &picture,
            sha256,
            root,
            recovery.clone(),
        )?);
    }
    Ok((job, images))
}

/// The output tokens `squint check` asks of each turn: the ceiling the judge of a
/// service holds by default (plan 2A, task S2), so that a turn is not cut off under
/// it.
pub const CHECK_OUTPUT_TOKENS: i64 = 2048;

/// A run's verdict as it was stored: its decision's word, its reason, and the items
/// of the reason whole, from the run's diagnostic of them, when it keeps one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stated {
    pub word: String,
    pub why: String,
    pub items: Vec<String>,
}

/// What reads a run's verdict back ([`Stated`]).
pub type ReadVerdict<'a> = dyn Fn(&RunId) -> Option<Stated> + 'a;

/// Runs `check` through the executor under a use case, and waits for it. A run that
/// completed prints its line, then its verdict, which `read` reads back from the
/// stored run, and each item of its reason whole, on a line of its own, from the run's
/// diagnostic of them (plan 1D, task F1, item 7): escaped, and cut only where any text
/// a door prints is ([`printable`]). A run that keeps no such diagnostic has its
/// reason printed as its record holds it. Any other end is the complaint, escaped
/// already.
pub fn check(
    executor: &dyn Executor,
    asked: &CheckAsked<'_>,
    read: &ReadVerdict<'_>,
) -> Result<String, String> {
    let (job, images) = shorthand(asked)?;
    let measuring = Measuring {
        details: asked.details,
        detail: asked.detail,
        projection: asked.projection,
        tools: &[],
        second_look: "off",
    };
    check_job(
        executor,
        &job,
        &images,
        asked.judge,
        asked.request_id,
        &measuring,
        read,
    )
}

/// What `check` measures before the judge's first turn, and how a call of `align` by the
/// judge is answered (ADR 0029).
#[derive(Debug, Clone, Copy)]
pub struct Measuring<'a> {
    pub details: Details,
    pub detail: Option<&'a str>,
    pub projection: &'a str,
    /// More tools offered to the judge beside those `details` gives: tool files the run
    /// names (`--tool`; plan 2C, task X1, item 4), each offered only so.
    pub tools: &'a [String],
    /// Whether a submission that leaves a mandatory item tentative gets one more turn:
    /// `off`, `on` or `strict` (plan m3, task U1, item 4).
    pub second_look: &'a str,
}

/// Runs `check` through the executor under a job and its images, in its order, and
/// waits for it, as [`check`] does.
pub fn check_job(
    executor: &dyn Executor,
    job: &Job,
    images: &[ArtifactId],
    judge: Option<&str>,
    request_id: Option<&str>,
    measuring: &Measuring<'_>,
    read: &ReadVerdict<'_>,
) -> Result<String, String> {
    let call = check_call(job, images, judge, measuring);
    let (run, mut text, _) = completed(executor, call, request_id)?;
    said_verdict(&mut text, read(&run));
    Ok(text)
}

/// The call of a use case's recipe under a job and its images, in its order, as every door
/// makes it (plan m3, task Q1): the recipe of the job's use case
/// (`whizbang_squint::recipes`), whose step `details` measures first and whose step `judge`
/// is `check` with the output limit of [`CHECK_OUTPUT_TOKENS`]; the job in the contract
/// `job/1`; the judge when one is named; and what differs from the recipe's own steps as
/// its overrides (`set`): what is measured first, the tools offered, the projection and the
/// second look.
pub fn check_call(
    job: &Job,
    images: &[ArtifactId],
    judge: Option<&str>,
    measuring: &Measuring<'_>,
) -> Call {
    use whizbang_squint::recipes::{DETAILS_STEP, JUDGE_STEP};
    let mut call = Call::new(&job.use_case).argument("job", Value::text(&job.to_json()));
    for image in images {
        call = call.input("images", *image);
    }
    if let Some(judge) = judge {
        call = call.argument("judge", Value::text(judge));
    }
    let (tools, details) = measuring.details.tools_and_name();
    let mut offered = tools.to_owned();
    for tool in measuring.tools {
        offered.push(',');
        offered.push_str(tool);
    }
    let mut set: Vec<String> = Vec::new();
    if details != "first" {
        set.push(format!("{DETAILS_STEP}.details={details}"));
    }
    if let Some(detail) = measuring.detail {
        set.push(format!("{DETAILS_STEP}.detail={detail}"));
    }
    if offered != Details::First.tools_and_name().0 {
        set.push(format!("{JUDGE_STEP}.tools={offered}"));
    }
    if measuring.projection != "full" {
        set.push(format!("{JUDGE_STEP}.projection={}", measuring.projection));
    }
    if measuring.second_look != "off" {
        set.push(format!(
            "{JUDGE_STEP}.second_look={}",
            measuring.second_look
        ));
    }
    if !set.is_empty() {
        call = call.argument(whizbang_core::recipe::SET, Value::text(&set.join("\n")));
    }
    call
}

/// A run's verdict, as `squint check` prints it after the run's line: the word, then
/// each item of the reason on a line of its own, escaped.
pub fn said_verdict(text: &mut String, stated: Option<Stated>) {
    match stated {
        Some(stated) => {
            text.push_str(&format!("verdict: {}\n", printable(&stated.word)));
            if stated.items.is_empty() {
                text.push_str(&format!("reason: {}\n", printable(&stated.why)));
            }
            for item in &stated.items {
                text.push_str(&format!("reason: {}\n", printable(item)));
            }
        }
        None => text.push_str("verdict: none was recorded\n"),
    }
}

/// The run of the judge's step of a use case's recipe, as its record lists it: the last
/// step that is `check`. None for a run that is no recipe's.
fn judge_step(record: &whizbang_core::contract::RunRecord) -> Option<RunId> {
    (record.steps().iter())
        .rev()
        .find(|step| step.recipe().is_some() && step.action() == whizbang_squint::check::NAME)
        .and_then(|step| RunId::parse(step.run()).ok())
}

/// The run that judged in the run `run` in the store at `root`: the judge's step of a use
/// case's recipe, or the run itself when it is no recipe's (or its record is not read).
pub fn judged_run(root: &Path, run: &RunId) -> RunId {
    let found = || {
        let reader = Reader::open(root, Arc::new(RealFs)).ok()?;
        let generation = reader.snapshot().ok()?;
        let record = whizbang_core::contract::record::stored(&generation, run.as_str(), |entry| {
            reader.read(entry)
        })?
        .ok()?;
        judge_step(&record)
    };
    found().unwrap_or_else(|| run.clone())
}

/// The verdict of the run `run` in the store at `root`, as its record holds it, read
/// by a reader of its own: its decision's word and its reason, and the items of its
/// reason from the run's diagnostic of them.
pub fn verdict(root: &Path, run: &RunId) -> Option<Stated> {
    let reader = Reader::open(root, Arc::new(RealFs)).ok()?;
    let generation = reader.snapshot().ok()?;
    let record = whizbang_core::contract::record::stored(&generation, run.as_str(), |entry| {
        reader.read(entry)
    })?
    .ok()?;
    let (decision, why) = record.verdict()?;
    let word = serde_json::to_value(decision).ok()?;
    // A use case's recipe hands back its judge's verdict; the items of its reason are kept
    // by the judge's step (plan m3, task Q1).
    let judged = judge_step(&record);
    let run = judged.as_ref().unwrap_or(run);
    // The items, each a JSON string on a line of its own; a line that is not one is
    // left out, and a diagnostic that is not there gives none.
    let items = diagnostic(root, run, whizbang_squint::use_case::REASONS)
        .map(|bytes| {
            String::from_utf8_lossy(&bytes)
                .lines()
                .filter_map(|line| serde_json::from_str::<String>(line).ok())
                .collect()
        })
        .unwrap_or_default();
    Some(Stated {
        word: word.as_str()?.to_owned(),
        why: why.to_owned(),
        items,
    })
}

/// The plan a run was admitted under, in one line, when its action planned its call
/// (`run/6`): read from the run's record.
pub fn plan_of(root: &Path, run: &RunId) -> Option<String> {
    let reader = Reader::open(root, Arc::new(RealFs)).ok()?;
    let generation = reader.snapshot().ok()?;
    let read = |entry: &_| reader.read(entry);
    let stored = whizbang_core::contract::record::stored(&generation, run.as_str(), read);
    let record = stored?.ok()?;
    record.plan().map(|plan| plan.line())
}
