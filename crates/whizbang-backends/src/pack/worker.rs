//! The supervisor of the one worker process (plan 1C, D4 and D7; task P3a): it starts the
//! pack's Python at the first request and not before, says hello and hears the worker's,
//! reads the worker's error output without stopping, and stops it.
//!
//! Ported from squint's pack client (`crates/squint-tools/src/mcp/mod.rs` at the commit
//! `docs/sources.toml` pins), as decision D1 has it: a thread that reads the worker's
//! output; its error output read line by line, bounded and redacted; a child stopped by
//! closing its input, then killed and waited for. From rascal (`crates/rascal-mcp/src/
//! client/env.rs`) comes the child's environment: cleared, and given a short allow-list
//! from which every name of a key is taken out. Squint's JSON-RPC and MCP are not ported;
//! the frames are `pack/1` ([`super::proto`]).
//!
//! **How the worker is started.** The pack is not installed (`tool.uv.package = false`),
//! so the supervisor starts the interpreter it is given, by its absolute path, isolated
//! and writing no bytecode (`-I -B`), with a short program of its own ([`BOOT`]) that puts
//! the pack's source on the Python path and runs the entry point as a program:
//! `<python> -I -B -c <boot> <source> <module> --models-dir <folder> --max-lifetime-s
//! <seconds>`. The lifetime is always passed. An interpreter is never looked for on
//! `PATH`: one that is not an absolute path to a file is refused before anything starts,
//! with a sentence that says to run `whizbang setup`, which makes the environment the
//! interpreter is in (task Y1).
//!
//! **What the worker is handed.** Its three standard streams, each a pipe, and nothing
//! else. Every file whizbang opens is closed where a child's program starts (the
//! standard library opens each so, and the workspace forbids the `unsafe` that could
//! take that off), and nothing runs between the start of the child and the start of its
//! program. A descriptor that whizbang was itself handed by whoever started it, and that
//! is not closed so, would reach the worker: the boot closes every descriptor above the
//! three before anything of the pack runs. A store's lock is never the worker's
//! (`whizbang_core::store` on the child's copy; the moment Ctrl-C can end whizbang while
//! a child is being started is accepted in `docs/carry-over.md`).
//!
//! **Its environment** is cleared, then given the variables of [`PASSED`] that the
//! parent has, but for any whose name is a key's (it ends in `_KEY`, `_TOKEN` or
//! `_SECRET`, or the config names it as a key), and then the variables of
//! [`environment`]: no token, no home folder and no proxy of the parent's passes. The
//! worker's home, its temporary folder, and the places where the Hub's libraries look for
//! their data and for the token of a sign-in (`HF_HOME`, `HF_TOKEN_PATH`,
//! `XDG_CACHE_HOME`) are one empty folder the supervisor makes for each worker and
//! removes once the worker has been waited for, so no library in the worker can find the
//! sign-in's store (D12, open question 9).
//!
//! **Its error output** is read by a thread of its own until it closes, line by line:
//! each line is redacted and cut to [`LINE_BYTES`], and the last [`LINES_KEPT`] are kept;
//! the rest are read and dropped, never left in the pipe, where a worker that prints
//! would block on a full pipe. A fault the supervisor makes is a sentence of fixed words
//! and numbers and quotes nothing of it.
//!
//! **Every wait is finite, on the clock it is given** (D7): 60 seconds for the hello, 5
//! for the worker to end once its input is closed; past either it is killed and waited
//! for. Each wait is also a bounded count of looks, so a clock that stands still ends it
//! too. The three threads are joined once the worker has been waited for: each ends at
//! the end of its pipe, which the end of the process closes. A process the worker started
//! that outlives it holds the pipes open: the readers are then given [`END_LIMIT`] and
//! let go past it, no worker is started until they end, and the kept error output ends
//! with [`LINGERING`]. Such a process is not killed: ending a worker's group of
//! processes takes a call the standard library does not have (`docs/carry-over.md`).
//! The worker starts none (task P2); whether the runtime does is task R1's to find.
//!
//! **Every path out of a start ends the worker,** a panic among them: from the moment
//! its process is started it is held by a value whose drop kills it, waits for it, lets
//! its readers end within [`END_LIMIT`] of the machine's time and removes its folder,
//! unless the supervisor's own end of it ran first.
//!
//! **One call at a time** (task P3b, [`Worker::call`]): a request is held to the codec's
//! rules before anything is sent, noted for the reader of the output, written, and
//! waited for on the clock, within the limit of its kind. Its end is its result, the
//! worker's `error` for it, or a failure of the worker, which ends the worker; the next
//! call starts another. A fault's sentence is fixed words and typed fields: nothing the
//! worker wrote, on its output or its error output, is in it. The reader holds what it
//! reads to what whizbang asked, hands a request of the worker's own on by its id
//! without reading its payload, and hands frames on through a channel of
//! [`FRAMES_WAITING`] places, waiting while it is full. Every frame is written by a
//! thread of its own, the writer of the worker's input, and the supervisor waits for it
//! on the clock: a request within its call's limit, every other frame within
//! [`END_LIMIT`]. A worker that reads nothing is killed past them, which ends the write
//! and the writer with it; such a call is `timed_out`. A worker that ended between two calls is noticed by the next; a call's own deadline,
//! cancellation, the queue and the restart's fault are task P4's.

use std::collections::VecDeque;
use std::ffi::OsString;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use whizbang_core::capability::{Fault, FaultKind};
use whizbang_core::contract::{Clock, Secrets};

use super::proto::{self, Answer, Ask, Message, Reason, Refused, Role};
use super::threads::{self, Spawn};

/// How long a worker has to start and say hello (D7).
pub const HELLO_LIMIT: Duration = Duration::from_secs(60);
/// How long a worker has to end once its input is closed (D7).
pub const END_LIMIT: Duration = Duration::from_secs(5);
/// How long a `load` has to end (D7).
pub const LOAD_LIMIT: Duration = Duration::from_secs(120);
/// How long an `unload` has to end (D7).
pub const UNLOAD_LIMIT: Duration = Duration::from_secs(30);
/// How long a `segment` has to end: the kind's limit for a call, 120 seconds by default
/// (D7). A call's own deadline, which may be shorter, is task P4's.
pub const CALL_LIMIT: Duration = Duration::from_secs(120);
/// How many frames the reader of the worker's output may have handed on that the
/// supervisor has not taken: past them the reader waits, and reads nothing more, until
/// one is taken. So what a worker that writes frames faster than they are taken can make
/// this process hold is these frames, each a header of at most 64 KiB but for the one
/// result of the one call, which its request bounds.
pub const FRAMES_WAITING: usize = 16;
/// How many lines of the worker's error output are kept: the last.
pub const LINES_KEPT: usize = 256;
/// How many bytes of one line of the error output are kept.
pub const LINE_BYTES: usize = 4_096;
/// How many bytes of one line are read before it is redacted; the rest of the line is
/// read and dropped. Twice what is kept, so that a secret that begins inside what is
/// kept is found whole, if it is no longer than 4 KiB.
const LINE_READ_BYTES: usize = 2 * LINE_BYTES;
/// How long the supervisor lets pass between two looks at what it waits for.
const PACE: Duration = Duration::from_millis(5);
/// The line the kept error output ends with when a worker's streams stayed open after
/// it ended, held by a process it started.
pub const LINGERING: &str = "whizbang: the worker ended and a process it started still holds its streams open; their readers were let go";
/// The entry point of the worker itself, which the kind `pack` starts.
pub const WORKER_MODULE: &str = "whizbang_pack.worker";

/// The variables of the parent's environment that the worker is handed when the parent
/// has them, but for a key's: the locale, and nothing else.
pub const PASSED: [&str; 3] = ["LANG", "LC_ALL", "LC_CTYPE"];

/// The threads each of the runtime's pools may hold (plan 1C, "Rules", "Threads in the
/// worker"), set before the runtime is imported.
const POOL_THREADS: &str = "8";

/// The allocator's high watermark on the Apple GPU, below 1.0 of the device's
/// recommended maximum (plan 1C, "Rules", "GPU memory"): task R1 starts at 0.5 and
/// writes here the value it ends with.
const GPU_WATERMARK: &str = "0.5";

/// The program the interpreter is started with (`-c`). It closes every descriptor
/// above its three, up to the process's limit of open files (a million when it has
/// none); imports what it uses of the standard library before the pack's source is on
/// the path, so that nothing in the pack can stand in for it; then puts the pack's
/// source (its first argument) on the Python path and runs the module its second
/// argument names as a program, with the arguments after those two.
pub const BOOT: &str = "import os, sys, resource, runpy\n\
most = resource.getrlimit(resource.RLIMIT_NOFILE)[0]\n\
if most == resource.RLIM_INFINITY or most > 1048576:\n    most = 1048576\n\
os.closerange(3, most)\n\
sys.path.insert(0, sys.argv.pop(1))\n\
name = sys.argv.pop(1)\n\
runpy.run_module(name, run_name='__main__', alter_sys=True)\n";

/// What the supervisor starts: the interpreter, the pack's source, the entry point,
/// and what the worker is told on its command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    /// The interpreter: an absolute path to a file. Never looked for on `PATH`.
    pub python: PathBuf,
    /// The pack's `src` folder, put on the interpreter's path.
    pub source: PathBuf,
    /// The entry point, run as a program: [`WORKER_MODULE`] for the kind `pack`.
    pub module: String,
    /// The models directory the worker opens installations in.
    pub models_dir: PathBuf,
    /// The worker's lifetime, `--max-lifetime-s`: always passed, at least 1.
    pub lifetime_s: u32,
    /// More of the command line, after the rest: the faults of the tests' fakes.
    pub more: Vec<String>,
}

impl Launch {
    /// The worker itself, as the kind `pack` starts it.
    pub fn worker(
        python: PathBuf,
        source: PathBuf,
        models_dir: PathBuf,
        lifetime_s: u32,
    ) -> Launch {
        Launch {
            python,
            source,
            module: WORKER_MODULE.to_owned(),
            models_dir,
            lifetime_s,
            more: Vec::new(),
        }
    }
}

/// How a worker ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    /// It ended by itself, with this exit code: 0 at the end of its input, 2 at a
    /// refused frame or command line, 3 at its lifetime (task P2).
    Exited(i32),
    /// The supervisor killed it, at a deadline or at once.
    Killed,
    /// A signal nobody here sent ended it.
    Signalled,
}

impl Ended {
    fn of(status: ExitStatus) -> Ended {
        status.code().map_or(Ended::Signalled, Ended::Exited)
    }

    /// What the exit code says, in fixed words.
    fn meaning(self) -> String {
        match self {
            Ended::Exited(0) => "it ended at the end of its input (exit code 0)".to_owned(),
            Ended::Exited(2) => "it refused a frame or its command line (exit code 2)".to_owned(),
            Ended::Exited(3) => "it reached its lifetime (exit code 3)".to_owned(),
            Ended::Exited(4) => "it ended inside a call (exit code 4)".to_owned(),
            Ended::Exited(code) => format!("it ended with exit code {code}"),
            Ended::Killed => "it was killed".to_owned(),
            Ended::Signalled => "a signal ended it".to_owned(),
        }
    }
}

/// What a call that was answered hands back (task P3b): the worker's result, and what
/// the call said beside it. The predicted IoU of each instance is in the answer
/// ([`proto::Found::iou`]); the result's diagnostics and the call's events are texts of
/// the protocol, in the adapter's words. None of these is a fault's sentence, and the
/// core's response holds none of them: they are its caller's to keep as a run's
/// diagnostics (`docs/carry-over.md`).
/// What `{:?}` prints of it is the device, the threads, how many bytes of diagnostics,
/// how many events and the answer's instances by their numbers: no word the worker
/// wrote.
#[derive(Clone, PartialEq)]
pub struct Called {
    /// The device that answered.
    pub device: String,
    /// How many threads the worker's process held after the call.
    pub threads: u32,
    /// The result's diagnostics.
    pub diagnostics: String,
    /// The text of each event the call was sent, in order.
    pub events: Vec<String>,
    /// The answer, as the wire held it.
    pub answer: Answer,
}

/// What a call that failed hands back (task P3b): its fault, whose sentence is fixed
/// words and typed fields and quotes nothing the worker wrote; the worker's own
/// sentence, when it answered with an `error`; and its error output, as kept. What
/// `{:?}` prints of it is the fault, how the worker ended, and only whether the worker
/// gave a sentence and how many lines of error output were kept: no word the worker
/// wrote, so a careless `{:?}` cannot carry one into a sentence.
#[derive(Clone, PartialEq)]
pub struct Failed {
    /// The fault the call ends in, with how far its request got.
    pub fault: Fault,
    /// The worker's sentence, when it ended the request with an `error`: a text of the
    /// protocol, for a run's diagnostics, never a fault's sentence.
    pub said: Option<String>,
    /// How the worker ended, when the failure ended it. Its error output is then whole.
    pub ended: Option<Ended>,
    /// The worker's error output as kept ([`Worker::error_output`]), redacted: taken
    /// once the worker has ended and its reader with it, when the failure ended it, and
    /// otherwise what was read so far.
    pub error_output: Vec<String>,
}

impl std::fmt::Debug for Called {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let answer = match &self.answer {
            Answer::Load => "load".to_owned(),
            Answer::Unload => "unload".to_owned(),
            Answer::Segment(found) => format!("segment of {} instances", found.len()),
        };
        f.debug_struct("Called")
            .field("device", &self.device)
            .field("threads", &self.threads)
            .field("diagnostics_bytes", &self.diagnostics.len())
            .field("events", &self.events.len())
            .field("answer", &answer)
            .finish()
    }
}

impl std::fmt::Debug for Failed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Failed")
            .field("fault", &self.fault)
            .field("said", &self.said.is_some())
            .field("ended", &self.ended)
            .field("error_output_lines", &self.error_output.len())
            .finish()
    }
}

/// What a worker said of itself in its hello.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Said {
    /// The device a model's work runs on; none while the worker has not asked its
    /// runtime.
    pub device: Option<String>,
    /// How many threads the worker's process held.
    pub threads: u32,
}

/// What the thread that reads the worker's output hands on: each message, a request of
/// the worker's own by its id alone (its payload unread), the end of the output, or a
/// refusal. Only a message is followed by more.
#[derive(Debug)]
enum Heard {
    Said(Message),
    Asked(String),
    Ended,
    Refused(Refused),
}

/// What the supervisor tells the reader of the worker's output before it writes a
/// request: its id, and what its answer is held to.
type Note = (String, proto::Awaited);

/// The lines of the error output that are kept.
type Kept = Arc<Mutex<VecDeque<String>>>;

/// One frame for the writer of the worker's input: the message, where the count of its
/// bytes the pipe took is kept, and where the writer says whether it was written whole.
struct Job {
    message: Message,
    taken: Arc<AtomicU64>,
    done: mpsc::Sender<bool>,
}

/// How many frames may wait for the writer of the worker's input. One call at a time
/// writes one request, and a word or two after it; a frame that finds no place is not
/// written.
const JOBS_WAITING: usize = 4;

/// The channel the reader of the worker's output hands frames on: of [`FRAMES_WAITING`]
/// places, so that the reader waits once they are full.
fn frames() -> (SyncSender<Heard>, Receiver<Heard>) {
    mpsc::sync_channel(FRAMES_WAITING)
}

/// A worker that is running, from the moment its process is started. Whatever path it
/// leaves by, a panic among them, its process is killed unless it ended and is waited
/// for, its readers are joined within [`END_LIMIT`] or let go, and its folder removed:
/// by [`Worker::finish`] on the paths that return, and by its drop on any other.
struct Running {
    child: Child,
    /// How the process ended, once it was waited for.
    reaped: Option<Ended>,
    /// The frames for the writer of its input, until its input is closed: then the
    /// writer writes what is queued and ends, which closes the input. A write the worker
    /// does not take is ended by the worker's end, which the kill makes sure of.
    input: Option<SyncSender<Job>>,
    /// The frames the reader of its output hands on, until the worker is killed: then
    /// the channel is let go, so that a reader that waits to hand one on ends.
    heard: Option<Receiver<Heard>>,
    /// Where the requests whizbang writes are noted for the reader, before each is
    /// written.
    notes: mpsc::Sender<Note>,
    /// The id of the conversation: whizbang's hello's.
    conversation: String,
    readers: Vec<JoinHandle<()>>,
    /// The worker's folder, until it is removed.
    home: Option<PathBuf>,
}

impl Running {
    /// The process ended by itself, with this status, and was waited for.
    fn reaped(&mut self, status: ExitStatus) -> Ended {
        let ended = Ended::of(status);
        self.reaped = Some(ended);
        ended
    }

    /// Kills the process, unless it was waited for or has ended, and waits for it; and
    /// lets go of the frames its reader hands on, so that a reader waiting to hand one
    /// on ends.
    fn kill(&mut self) -> Ended {
        drop(self.input.take());
        let ended = match self.reaped {
            Some(ended) => ended,
            None => kill(&mut self.child),
        };
        self.reaped = Some(ended);
        drop(self.heard.take());
        ended
    }

    /// The next thing the reader of the output handed on, if one is there: `Some(None)`
    /// once the reader is gone.
    fn next(&self) -> Option<Option<Heard>> {
        match self.heard.as_ref().map(Receiver::try_recv) {
            Some(Ok(heard)) => Some(Some(heard)),
            Some(Err(TryRecvError::Empty)) => None,
            Some(Err(TryRecvError::Disconnected)) | None => Some(None),
        }
    }

    /// Hands a frame to the writer of the worker's input, waiting for nothing: where the
    /// writer will say whether it was written whole, or nothing when it could not be
    /// queued (the input is closed, the writer has ended, or [`JOBS_WAITING`] frames
    /// wait already).
    fn queue(&self, message: Message, taken: &Arc<AtomicU64>) -> Option<Receiver<bool>> {
        let (done, written) = mpsc::channel();
        let job = Job {
            message,
            taken: taken.clone(),
            done,
        };
        self.input.as_ref()?.try_send(job).ok()?;
        Some(written)
    }

    /// Removes the worker's folder, once.
    fn remove_home(&mut self) {
        if let Some(home) = self.home.take() {
            let _ = std::fs::remove_dir_all(home);
        }
    }
}

impl Drop for Running {
    /// The end of a worker on a path that did not reach [`Worker::finish`]: a panic
    /// between its start and its end. The process is killed and waited for; the readers
    /// are given [`END_LIMIT`] of the machine's time, not the supervisor's clock, which
    /// may be what panicked, and are let go past it; the folder is removed.
    fn drop(&mut self) {
        self.kill();
        let until = std::time::Instant::now() + END_LIMIT;
        while self.readers.iter().any(|reader| !reader.is_finished())
            && std::time::Instant::now() < until
        {
            std::thread::sleep(PACE);
        }
        for reader in self.readers.drain(..) {
            if reader.is_finished() {
                let _ = reader.join();
            }
        }
        self.remove_home();
    }
}

/// The supervisor of one worker process. It starts none until it is asked to
/// ([`Worker::ready`]), and stops the one it started when it is dropped.
pub struct Worker {
    launch: Launch,
    clock: Arc<dyn Clock>,
    secrets: Secrets,
    keys: Vec<String>,
    parent: Vec<(OsString, OsString)>,
    spawn: Spawn,
    running: Option<Running>,
    said: Option<Said>,
    kept: Kept,
    ended: Option<Ended>,
    conversations: u64,
    /// How many requests were made: each request's id is `r` and its number.
    requests: u64,
    /// The readers of a worker that ended while something held its streams open: let
    /// go at [`END_LIMIT`], and no worker is started while one of them lives.
    lingering: Vec<JoinHandle<()>>,
}

impl std::fmt::Debug for Worker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Worker")
            .field("launch", &self.launch)
            .field("running", &self.running.as_ref().map(|r| r.child.id()))
            .field("ended", &self.ended)
            .finish_non_exhaustive()
    }
}

impl Worker {
    /// A supervisor of the worker this launches, on this clock, that redacts these
    /// secrets from the error output it keeps. It starts nothing. The environment the
    /// worker's is chosen from is this process's.
    pub fn new(launch: Launch, clock: Arc<dyn Clock>, secrets: Secrets) -> Worker {
        Worker {
            launch,
            clock,
            secrets,
            keys: Vec::new(),
            parent: std::env::vars_os().collect(),
            spawn: threads::spawn,
            running: None,
            said: None,
            kept: Arc::default(),
            ended: None,
            conversations: 0,
            requests: 0,
            lingering: Vec::new(),
        }
    }

    /// The same, where the config names these variables as keys: none of them is
    /// handed to the worker, whatever its name.
    pub fn with_keys(mut self, keys: impl IntoIterator<Item = String>) -> Worker {
        self.keys = keys.into_iter().collect();
        self
    }

    /// The same, choosing the worker's environment from these variables in place of
    /// this process's.
    pub fn with_parent_environment(
        mut self,
        parent: impl IntoIterator<Item = (OsString, OsString)>,
    ) -> Worker {
        self.parent = parent.into_iter().collect();
        self
    }

    /// The same, making its three threads with this: a test's, counted with the test's
    /// other threads.
    pub fn with_spawn(mut self, spawn: Spawn) -> Worker {
        self.spawn = spawn;
        self
    }

    /// What a request does first: starts the worker if none is running, and waits for
    /// its hello. A worker that is not started, says no hello within [`HELLO_LIMIT`],
    /// speaks another version or ends first is killed if it lives, waited for, and the
    /// request ends with a fault that says so, sent to nobody.
    pub fn ready(&mut self) -> Result<(), Fault> {
        if self.running.is_some() {
            return Ok(());
        }
        let (ended, lingering): (Vec<_>, Vec<_>) = std::mem::take(&mut self.lingering)
            .into_iter()
            .partition(JoinHandle::is_finished);
        for reader in ended {
            let _ = reader.join();
        }
        self.lingering = lingering;
        if !self.lingering.is_empty() {
            return Err(refusal(
                FaultKind::Unavailable,
                "a process the last worker started still holds its streams open; no worker is started until they close",
            ));
        }
        let python = &self.launch.python;
        if !python.is_absolute() || !python.is_file() {
            return Err(refusal(
                FaultKind::Unavailable,
                "the pack's Python is not installed; run whizbang setup",
            ));
        }
        let running = self.start()?;
        self.hear_hello(running)
    }

    /// The process id of the worker that is running, if one is.
    pub fn pid(&self) -> Option<u32> {
        self.running.as_ref().map(|running| running.child.id())
    }

    /// The folder of the worker that is running: its home, its temporary folder and the
    /// Hub's libraries' place, empty when it was started, removed when it ends.
    pub fn folder(&self) -> Option<&Path> {
        self.running
            .as_ref()
            .and_then(|running| running.home.as_deref())
    }

    /// What the worker that is running said of itself in its hello.
    pub fn said(&self) -> Option<&Said> {
        self.running.as_ref().and(self.said.as_ref())
    }

    /// How the last worker ended, once it has.
    pub fn ended(&self) -> Option<Ended> {
        self.ended
    }

    /// The lines of the last worker's error output that are kept, redacted, oldest
    /// first: of the one running, or of the one that ended last.
    pub fn error_output(&self) -> Vec<String> {
        let kept = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
        kept.iter().cloned().collect()
    }

    /// Stops the worker that is running, as D7 says: closes its input, lets it end
    /// within [`END_LIMIT`], or kills it, and waits for it either way. Says how it
    /// ended; nothing when none was running.
    pub fn stop(&mut self) -> Option<Ended> {
        let running = self.running.take()?;
        Some(self.close(running))
    }

    /// Kills the worker that is running now, and waits for it. Says how it ended;
    /// nothing when none was running.
    pub fn kill(&mut self) -> Option<Ended> {
        let running = self.running.take()?;
        Some(self.finish(running))
    }

    /// One request and its end (task P3b). A request the codec would refuse to write is
    /// a fault at once, `not_sent`, and no worker is started for it nor told anything.
    /// Otherwise the worker is started if none runs ([`Worker::ready`]), the request is
    /// noted for the reader of its output and written, and the call waits, on the
    /// clock, for its result: its events are kept as they come.
    ///
    /// A call ends in its result; in the worker's `error` for it, a fault of the kind
    /// the worker named, answered, and the worker lives on; or in a failure of the
    /// worker, which ends it: a frame refused (`too_large` when the bound broken is a
    /// header's, a result's payloads' or a count of its instances, and `malformed`
    /// otherwise: ADR 0020, "What a refusal says"), a request of the worker's own, a
    /// frame that is no part of a call, the end of its output, or no end within the
    /// call's limit ([`LOAD_LIMIT`], [`UNLOAD_LIMIT`], [`CALL_LIMIT`]), past which it is
    /// killed (`timed_out`). A worker that refused nothing is told why in an `error` that
    /// ends the conversation, and is given [`END_LIMIT`] to end before it is killed. The
    /// next call starts another. A request any byte of which was written and that no
    /// answer settled is `unresolved`; one of which no byte was written is `not_sent`.
    pub fn call(&mut self, ask: Ask) -> Result<Called, Box<Failed>> {
        let limit = match &ask {
            Ask::Load { .. } => LOAD_LIMIT,
            Ask::Unload { .. } => UNLOAD_LIMIT,
            Ask::Segment { .. } => CALL_LIMIT,
        };
        let awaited = proto::Awaited::of(&ask);
        self.requests += 1;
        let id = format!("r{}", self.requests);
        let request = Message::Request {
            id: id.clone(),
            ask,
        };
        if let Err(refused) = proto::writable(&request) {
            let kind = match refused.reason() {
                Reason::HeaderTooLarge | Reason::OverBound | Reason::TooLarge => {
                    FaultKind::TooLarge
                }
                _ => FaultKind::Rejected,
            };
            let sentence = format!(
                "the request breaks a rule of the protocol ({}) and was not sent to the pack's worker",
                refused.reason().name()
            );
            return Err(self.failed(refusal(kind, &sentence), None, None));
        }
        let started = self.conversations;
        if let Err(fault) = self.ready() {
            let ended = (self.conversations != started)
                .then_some(self.ended)
                .flatten();
            return Err(self.failed(fault, None, ended));
        }
        let Some(running) = self.running.take() else {
            let fault = refusal(FaultKind::Unavailable, "the pack's worker is not running");
            return Err(self.failed(fault, None, None));
        };
        // Noted before a byte is written, so that the reader holds the answer to it. The
        // writer writes it; the call waits for that, and then for the answer, within its
        // limit on the clock.
        let _ = running.notes.send((id.clone(), awaited));
        let taken = Arc::new(AtomicU64::new(0));
        let stage = |fault: Fault, taken: &AtomicU64| match taken.load(Ordering::SeqCst) {
            0 => fault.not_sent(),
            _ => fault.unresolved(),
        };
        let Some(written) = running.queue(request, &taken) else {
            let ended = self.close(running);
            let words = "the request could not be handed to the writer of the pack's worker, which was ended";
            let fault = stage(Fault::new(FaultKind::Unavailable, words), &taken);
            return Err(self.failed(fault, None, Some(ended)));
        };
        let mut wrote = None;
        let mut events = Vec::new();
        let waited = self.until(limit, || {
            loop {
                match running.next() {
                    Some(Some(Heard::Said(Message::Event { id: of, text }))) if of == id => {
                        events.push(text);
                    }
                    Some(heard) => return Some(Waited::Heard(heard)),
                    None => break,
                }
            }
            if wrote.is_none() {
                wrote = match written.try_recv() {
                    Ok(whole) => Some(whole),
                    Err(TryRecvError::Empty) => None,
                    Err(TryRecvError::Disconnected) => Some(false),
                };
            }
            (wrote == Some(false)).then_some(Waited::Unwritten)
        });
        let heard = match waited {
            Some(Waited::Heard(heard)) => Some(heard),
            Some(Waited::Unwritten) => {
                let ended = self.close(running);
                let words =
                    "the request could not be written to the pack's worker, which was ended";
                let fault = stage(Fault::new(FaultKind::Unavailable, words), &taken);
                return Err(self.failed(fault, None, Some(ended)));
            }
            None => None,
        };
        let conversation = running.conversation.clone();
        let (kind, sentence) = match heard {
            Some(Some(Heard::Said(Message::Result {
                id: of,
                device,
                threads,
                diagnostics,
                answer,
            }))) if of == id => {
                self.running = Some(running);
                return Ok(Called {
                    device,
                    threads,
                    diagnostics,
                    events,
                    answer,
                });
            }
            Some(Some(Heard::Said(Message::Error {
                id: of,
                fault,
                sentence,
            }))) if of == id => {
                self.running = Some(running);
                let words = format!(
                    "the pack's worker ended the request with the fault {}",
                    fault.name()
                );
                let fault = Fault::new(fault, &words).answered(None);
                return Err(self.failed(fault, Some(sentence), None));
            }
            None => {
                // The kill ends a write the worker did not take, and the writer with it.
                let ended = self.finish(running);
                let words = format!(
                    "the pack's worker did not end the request within {} seconds and was killed",
                    limit.as_secs()
                );
                let fault = stage(Fault::new(FaultKind::TimedOut, &words), &taken);
                return Err(self.failed(fault, None, Some(ended)));
            }
            Some(Some(Heard::Said(Message::Error { id: of, .. }))) if of == conversation => {
                let ended = self.close(running);
                let words = format!(
                    "the pack's worker ended the conversation inside the call: {}",
                    ended.meaning()
                );
                let fault = Fault::new(FaultKind::Unavailable, &words).unresolved();
                return Err(self.failed(fault, None, Some(ended)));
            }
            Some(Some(Heard::Ended) | None) => {
                let ended = self.close(running);
                let words = format!(
                    "the pack's worker ended inside the call: {}",
                    ended.meaning()
                );
                let fault = Fault::new(FaultKind::Unavailable, &words).unresolved();
                return Err(self.failed(fault, None, Some(ended)));
            }
            Some(Some(Heard::Refused(refused))) => {
                let kind = fault_of(&refused);
                self.end_conversation(&running, kind, refused.sentence());
                let sentence = format!(
                    "the pack's worker answered with a frame that was refused ({}); it was ended",
                    refused.reason().name()
                );
                (kind, sentence)
            }
            Some(Some(Heard::Asked(asked))) => {
                let answer = Message::Error {
                    id: asked,
                    fault: FaultKind::Unsupported,
                    sentence: proto::sentence("a worker asks nothing of whizbang at this version"),
                };
                self.tell(&running, answer);
                self.end_conversation(
                    &running,
                    FaultKind::Malformed,
                    "a worker asks nothing of whizbang at this version, and its request was not read",
                );
                let sentence = "the pack's worker sent a request of its own, which this version does not take; it was ended";
                (FaultKind::Malformed, sentence.to_owned())
            }
            Some(Some(Heard::Said(_))) => {
                self.end_conversation(
                    &running,
                    FaultKind::Malformed,
                    "a frame that has no place in a call",
                );
                let sentence =
                    "the pack's worker sent a frame that has no place in a call; it was ended";
                (FaultKind::Malformed, sentence.to_owned())
            }
        };
        let ended = self.close(running);
        let fault = Fault::new(kind, &sentence).unresolved();
        Err(self.failed(fault, None, Some(ended)))
    }

    /// Writes a frame on the worker's input through its writer, and waits for it within
    /// [`END_LIMIT`] on the clock: whether it was written whole by then. A frame the
    /// worker does not take is left to the worker's end, which follows every word told.
    fn tell(&self, running: &Running, message: Message) -> bool {
        let taken = Arc::new(AtomicU64::new(0));
        let Some(written) = running.queue(message, &taken) else {
            return false;
        };
        let whole = self.until(END_LIMIT, || match written.try_recv() {
            Ok(whole) => Some(whole),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(false),
        });
        whole == Some(true)
    }

    /// Tells the worker the conversation is over, in an `error` of the conversation's
    /// id whose sentence is a text of the protocol made of these words.
    fn end_conversation(&self, running: &Running, fault: FaultKind, words: &str) {
        let error = Message::Error {
            id: running.conversation.clone(),
            fault,
            sentence: proto::sentence(words),
        };
        self.tell(running, error);
    }

    /// A failed call: its fault, the worker's own sentence when it gave one, how the
    /// worker ended when the failure ended it, and the error output as kept now, which
    /// is whole once the worker has ended, since its end waits for the reader's.
    fn failed(&self, fault: Fault, said: Option<String>, ended: Option<Ended>) -> Box<Failed> {
        Box::new(Failed {
            fault,
            said,
            ended,
            error_output: self.error_output(),
        })
    }

    /// The end of a worker that is let end by itself: its input closed, [`END_LIMIT`]
    /// of the clock for it to end, then killed, and waited for either way.
    fn close(&mut self, mut running: Running) -> Ended {
        drop(running.input.take());
        let exited = self.until(END_LIMIT, || running.child.try_wait().ok().flatten());
        if let Some(status) = exited {
            running.reaped(status);
        }
        self.finish(running)
    }

    /// Starts the process and its three threads.
    fn start(&mut self) -> Result<Running, Fault> {
        let home = make_home().map_err(|_| {
            refusal(
                FaultKind::Unavailable,
                "the pack's worker could not be given a folder",
            )
        })?;
        let mut command = Command::new(&self.launch.python);
        command
            .args(["-I", "-B", "-c", BOOT])
            .arg(&self.launch.source)
            .arg(&self.launch.module)
            .arg("--models-dir")
            .arg(&self.launch.models_dir)
            .arg("--max-lifetime-s")
            .arg(self.launch.lifetime_s.to_string())
            .args(&self.launch.more)
            .env_clear()
            .envs(environment(&self.parent, &self.keys, &home))
            .current_dir(&home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(_) => {
                let _ = std::fs::remove_dir_all(&home);
                return Err(refusal(
                    FaultKind::Unavailable,
                    "the pack's worker could not be started",
                ));
            }
        };
        let (input, output, errors) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take());
        self.kept = Arc::default();
        self.said = None;
        self.conversations += 1;
        let (tell, heard) = frames();
        let (notes, noted) = mpsc::channel();
        let (jobs, queued) = mpsc::sync_channel(JOBS_WAITING);
        // From here on the worker is ended on every path, a panic among them, by
        // `finish` or by the drop of `running`.
        let mut running = Running {
            child,
            reaped: None,
            input: Some(jobs),
            heard: Some(heard),
            notes,
            conversation: format!("c{}", self.conversations),
            readers: Vec::new(),
            home: Some(home),
        };
        let made = match (input, output, errors) {
            (Some(input), Some(output), Some(errors)) => {
                let kept = (self.kept.clone(), self.secrets.clone());
                (self.spawn)(
                    "whizbang-pack-output".to_owned(),
                    Box::new(move || read_output(output, &tell, &noted)),
                )
                .and_then(|reader| {
                    running.readers.push(reader);
                    (self.spawn)(
                        "whizbang-pack-errors".to_owned(),
                        Box::new(move || read_error_output(errors, &kept.0, &kept.1)),
                    )
                })
                .and_then(|reader| {
                    running.readers.push(reader);
                    (self.spawn)(
                        "whizbang-pack-input".to_owned(),
                        Box::new(move || write_input(input, &queued)),
                    )
                })
                .map(|writer| running.readers.push(writer))
            }
            _ => Err(io::Error::other("a stream of the worker was not piped")),
        };
        if made.is_err() {
            self.finish(running);
            return Err(refusal(
                FaultKind::Unavailable,
                "the pack's worker could not be given its threads; a process an earlier worker started may still hold that worker's streams open",
            ));
        }
        Ok(running)
    }

    /// Says hello, and waits for the worker's. A worker whose answer is refused, or is
    /// no hello, is told why in an `error` that ends the conversation, and is given
    /// [`END_LIMIT`] to end before it is killed; one that says nothing within
    /// [`HELLO_LIMIT`] is killed.
    fn hear_hello(&mut self, running: Running) -> Result<(), Fault> {
        let conversation = running.conversation.clone();
        // A worker that cannot be written to has ended, or will: what its output says
        // next tells how.
        let hello = Message::Hello {
            id: conversation.clone(),
            role: Role::Whizbang,
        };
        self.tell(&running, hello);
        let heard = self.until(HELLO_LIMIT, || running.next());
        let (kind, sentence, told) = match heard {
            Some(Some(Heard::Said(Message::Hello {
                id,
                role: Role::Worker { device, threads },
            }))) if id == conversation => {
                self.said = Some(Said { device, threads });
                self.running = Some(running);
                return Ok(());
            }
            Some(Some(Heard::Refused(refused))) if refused.reason() == Reason::Version => (
                FaultKind::Unavailable,
                "the pack's worker speaks another version of the protocol than this whizbang",
                Some(refused.sentence().to_owned()),
            ),
            Some(Some(Heard::Refused(refused))) => (
                FaultKind::Malformed,
                "the pack's worker answered its hello with a frame that was refused",
                Some(refused.sentence().to_owned()),
            ),
            Some(Some(Heard::Said(Message::Error { id, .. }))) if id == conversation => (
                FaultKind::Unavailable,
                "the pack's worker ended the conversation at its hello",
                None,
            ),
            Some(Some(Heard::Said(_) | Heard::Asked(_))) => (
                FaultKind::Malformed,
                "the pack's worker answered its hello with another message",
                Some("the first frame of a worker is its hello".to_owned()),
            ),
            Some(Some(Heard::Ended) | None) => {
                // Its output ended: the worker has ended, or is ending.
                let ended = self.close(running);
                return Err(refusal(
                    FaultKind::Unavailable,
                    &format!(
                        "the pack's worker ended before its hello: {}",
                        ended.meaning()
                    ),
                ));
            }
            None => {
                self.finish(running);
                return Err(refusal(
                    FaultKind::Unavailable,
                    &format!(
                        "the pack's worker said no hello within {} seconds and was killed",
                        HELLO_LIMIT.as_secs()
                    ),
                ));
            }
        };
        if let Some(words) = told {
            self.end_conversation(&running, kind, &words);
        }
        self.close(running);
        Err(refusal(kind, sentence))
    }

    /// The end of a worker: its process killed unless it ended and was waited for; its
    /// two readers joined, each at the end of its pipe, which the end of the process
    /// closes, within [`END_LIMIT`] of the clock; its folder removed; and how it ended
    /// noted. A process the worker started that holds its streams open past that keeps
    /// the readers from their end: they are let go, kept as lingering, which refuses the
    /// next start until they end, and a line of the kept error output says so. Nothing
    /// that was started is killed but the worker (the end of a worker's group of
    /// processes is not one the standard library gives; `docs/carry-over.md`).
    fn finish(&mut self, mut running: Running) -> Ended {
        let ended = running.kill();
        let all_ended = self
            .until(END_LIMIT, || {
                running
                    .readers
                    .iter()
                    .all(JoinHandle::is_finished)
                    .then_some(())
            })
            .is_some();
        for reader in running.readers.drain(..) {
            if reader.is_finished() {
                let _ = reader.join();
            } else {
                self.lingering.push(reader);
            }
        }
        if !all_ended {
            let mut kept = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
            if kept.len() == LINES_KEPT {
                kept.pop_front();
            }
            kept.push_back(LINGERING.to_owned());
        }
        running.remove_home();
        self.ended = Some(ended);
        ended
    }

    /// Looks until `look` finds something or `limit` of the clock has passed, letting
    /// [`PACE`] pass between two looks; and at most a bounded count of looks, so that a
    /// clock that stands still ends the wait too. One last look is made at the end.
    fn until<T>(&self, limit: Duration, mut look: impl FnMut() -> Option<T>) -> Option<T> {
        let start = self.clock.now();
        let until = start.checked_add(limit);
        let most = 2 * (limit.as_millis() / PACE.as_millis()).max(1);
        for _ in 0..most {
            if let Some(found) = look() {
                return Some(found);
            }
            if until.is_none_or(|until| self.clock.now() >= until) {
                break;
            }
            self.clock.pause(PACE);
        }
        look()
    }
}

impl Drop for Worker {
    /// Stops the worker that is running (D7), and waits for it.
    fn drop(&mut self) {
        self.stop();
    }
}

/// A fault the supervisor makes: fixed words, sent to nobody.
fn refusal(kind: FaultKind, sentence: &str) -> Fault {
    Fault::new(kind, sentence).not_sent()
}

/// Kills a child, unless it has ended already, and waits for it.
fn kill(child: &mut Child) -> Ended {
    if let Ok(Some(status)) = child.try_wait() {
        return Ended::of(status);
    }
    let _ = child.kill();
    let _ = child.wait();
    Ended::Killed
}

/// The worker's environment: the variables of [`PASSED`] that the parent has, but for a
/// key's, then the supervisor's own. `home` is the worker's empty folder.
pub fn environment(
    parent: &[(OsString, OsString)],
    keys: &[String],
    home: &Path,
) -> Vec<(OsString, OsString)> {
    let mut chosen: Vec<(OsString, OsString)> = parent
        .iter()
        .filter(|(name, _)| {
            let name = name.to_string_lossy();
            PASSED.contains(&name.as_ref()) && !is_key(&name, keys)
        })
        .cloned()
        .collect();
    let home = home.as_os_str().to_owned();
    let mut token = PathBuf::from(&home);
    token.push("token");
    let set: [(&str, OsString); 17] = [
        // The worker's own folder, empty, for everything that would look in a home.
        ("HOME", home.clone()),
        ("TMPDIR", home.clone()),
        ("XDG_CACHE_HOME", home.clone()),
        // The Hub's libraries: their data, and the token of a sign-in, in that folder,
        // where there is none; no call to the Hub at all; no usage data sent.
        ("HF_HOME", home.clone()),
        ("HF_TOKEN_PATH", token.into_os_string()),
        ("HF_HUB_OFFLINE", "1".into()),
        ("HF_HUB_DISABLE_TELEMETRY", "1".into()),
        ("HF_HUB_DISABLE_IMPLICIT_TOKEN", "1".into()),
        ("DO_NOT_TRACK", "1".into()),
        // The system's programs, and none of a home folder's.
        ("PATH", "/usr/bin:/bin".into()),
        // The runtime's pools, held before it is imported.
        ("OMP_NUM_THREADS", POOL_THREADS.into()),
        ("MKL_NUM_THREADS", POOL_THREADS.into()),
        ("OPENBLAS_NUM_THREADS", POOL_THREADS.into()),
        ("VECLIB_MAXIMUM_THREADS", POOL_THREADS.into()),
        ("TOKENIZERS_PARALLELISM", "false".into()),
        ("PYTORCH_MPS_HIGH_WATERMARK_RATIO", GPU_WATERMARK.into()),
        ("PYTHONDONTWRITEBYTECODE", "1".into()),
    ];
    for (name, value) in set {
        chosen.retain(|(had, _)| had != name);
        chosen.push((name.into(), value));
    }
    chosen
}

/// Whether a variable holds a key: its name ends in `_KEY`, `_TOKEN` or `_SECRET`, in
/// any case, or the config names it as a key.
pub fn is_key(name: &str, keys: &[String]) -> bool {
    let upper = name.to_ascii_uppercase();
    keys.iter().any(|key| key == name)
        || ["_KEY", "_TOKEN", "_SECRET"]
            .iter()
            .any(|end| upper.ends_with(end))
}

/// A new empty folder for one worker, for its owner only, in the system's temporary
/// folder.
fn make_home() -> io::Result<PathBuf> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let base = std::env::temp_dir();
    let mut last = io::Error::other("no folder was tried");
    for _ in 0..16 {
        let path = base.join(format!(
            "whizbang-worker-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        match builder.create(&path) {
            Ok(()) => return Ok(path),
            Err(error) => last = error,
        }
    }
    Err(last)
}

/// What a refusal of a frame from the worker ends its call as (ADR 0020, "What a
/// refusal says"): `too_large` when the bound broken is a header's, a result's payloads'
/// together, or a count of a result's instances; `malformed` for every other reason.
fn fault_of(refused: &Refused) -> FaultKind {
    match refused.reason() {
        Reason::HeaderTooLarge | Reason::TooLarge => FaultKind::TooLarge,
        Reason::OverBound if refused.counts_instances() => FaultKind::TooLarge,
        _ => FaultKind::Malformed,
    }
}

/// A writer that counts the bytes the one under it took: whether any byte of a request
/// left whizbang.
struct Counted<'a, W> {
    out: &'a mut W,
    count: &'a AtomicU64,
}

impl<W: Write> Write for Counted<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let taken = self.out.write(bytes)?;
        self.count.fetch_add(taken as u64, Ordering::SeqCst);
        Ok(taken)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

/// What a call waited for: the end its request was given, or that its request could not
/// be written.
enum Waited {
    Heard(Option<Heard>),
    Unwritten,
}

/// The thread that writes the worker's input: each frame it is handed, in order,
/// counting the bytes the pipe takes, and saying whether it was written whole. It ends
/// when the frames stop (the input is closed, or the worker killed), or at the first
/// write that fails, and its end closes the input. A write the worker does not take waits
/// here, not in the supervisor, until the worker ends: the supervisor's wait for it is
/// on the clock, and past it the worker is killed, which fails the write.
fn write_input(input: ChildStdin, jobs: &Receiver<Job>) {
    let mut input = input;
    for job in jobs.iter() {
        let whole = proto::write(
            &job.message,
            &mut Counted {
                out: &mut input,
                count: &job.taken,
            },
        )
        .is_ok();
        drop(job.message);
        let _ = job.done.send(whole);
        if !whole {
            return;
        }
    }
}

/// The thread that reads the worker's output: each frame, handed on, until the output
/// ends, a frame is refused, a request of the worker's own is heard, or nobody listens.
/// It hands a frame on through the channel of [`frames`], and waits while that is full.
///
/// What it holds the frames to is what whizbang asked: each request is noted on `notes`
/// before it is written, and the notes are taken once the first byte of the next frame
/// is there, so that every request the worker can be answering is among them. A
/// request from the worker is handed on by its id, its payload not read.
fn read_output(output: impl Read, tell: &SyncSender<Heard>, notes: &Receiver<Note>) {
    let mut output = BufReader::new(output);
    let mut open = proto::Open::new();
    loop {
        // Waits for the first byte of the next frame, or for the end; what it finds
        // is read again below.
        let _ = output.fill_buf();
        while let Ok((id, awaited)) = notes.try_recv() {
            let _ = open.expect(&id, awaited);
        }
        let heard = match proto::hear(&mut output, &mut open) {
            Ok(Some(proto::Heard::Message(message))) => Heard::Said(message),
            Ok(Some(proto::Heard::Asked { id })) => Heard::Asked(id),
            Ok(None) => Heard::Ended,
            Err(refused) => Heard::Refused(refused),
        };
        let last = !matches!(heard, Heard::Said(_));
        if tell.send(heard).is_err() || last {
            return;
        }
    }
}

/// The thread that reads the worker's error output, to its end, whatever it holds:
/// each line, without its line break, is redacted, cut to [`LINE_BYTES`] and kept, the
/// last [`LINES_KEPT`] of them. What is past [`LINE_READ_BYTES`] of a line is read and
/// dropped. Only a failed read ends it before the end.
fn read_error_output(errors: ChildStderr, kept: &Kept, secrets: &Secrets) {
    let mut errors = BufReader::new(errors);
    let mut line: Vec<u8> = Vec::new();
    let keep = |line: &mut Vec<u8>| {
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        let redacted = secrets.redact_bytes(line);
        let mut text = String::from_utf8_lossy(&redacted).into_owned();
        if text.len() > LINE_BYTES {
            let mut cut = LINE_BYTES;
            while !text.is_char_boundary(cut) {
                cut -= 1;
            }
            text.truncate(cut);
        }
        let mut kept = kept.lock().unwrap_or_else(PoisonError::into_inner);
        if kept.len() == LINES_KEPT {
            kept.pop_front();
        }
        kept.push_back(text);
        line.clear();
    };
    loop {
        let buffer = match errors.fill_buf() {
            Ok([]) => break,
            Ok(buffer) => buffer,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        let (piece, used, ends) = match buffer.iter().position(|&byte| byte == b'\n') {
            Some(at) => (&buffer[..at], at + 1, true),
            None => (buffer, buffer.len(), false),
        };
        let room = LINE_READ_BYTES.saturating_sub(line.len());
        line.extend_from_slice(&piece[..piece.len().min(room)]);
        errors.consume(used);
        if ends {
            keep(&mut line);
        }
    }
    if !line.is_empty() {
        keep(&mut line);
    }
}

#[cfg(test)]
mod tests {
    //! The reader of the worker's output, over input of the test's own: no worker is
    //! started here. One thread, the test kit's, which the test joins.
    use std::sync::atomic::AtomicUsize;

    use super::*;

    /// How many frames the input holds.
    const FRAMES: usize = 200;

    /// Input of [`FRAMES`] frames, one handed out by each read, as a worker that writes
    /// as fast as it can would hand them. Each read looks at how far it has got ahead of
    /// what the test took, and fails the reader's thread if it is further ahead than the
    /// frames that may wait, the one the reader holds and the one the test is taking.
    struct Flood {
        frame: Vec<u8>,
        given: usize,
        taken: Arc<AtomicUsize>,
    }

    impl Read for Flood {
        fn read(&mut self, into: &mut [u8]) -> io::Result<usize> {
            if self.given == FRAMES {
                return Ok(0);
            }
            let ahead = self.given - self.taken.load(Ordering::SeqCst);
            assert!(
                ahead <= FRAMES_WAITING + 2,
                "the reader read {ahead} frames ahead of what was taken"
            );
            assert!(into.len() >= self.frame.len());
            into[..self.frame.len()].copy_from_slice(&self.frame);
            self.given += 1;
            Ok(self.frame.len())
        }
    }

    #[test]
    fn the_reader_of_the_output_reads_no_further_ahead_than_the_frames_that_may_wait() {
        let mut frame = Vec::new();
        let cancel = Message::Cancel {
            id: "c1".to_owned(),
        };
        proto::write(&cancel, &mut frame).unwrap();
        let taken = Arc::new(AtomicUsize::new(0));
        let input = Flood {
            frame,
            given: 0,
            taken: taken.clone(),
        };
        let (tell, heard) = frames();
        let (_notes, noted) = mpsc::channel::<Note>();
        let reader = whizbang_testkit::threads::spawn(move || read_output(input, &tell, &noted));
        // The test takes the frames one at a time, each after the reader handed it on.
        let mut said = 0;
        while let Ok(next) = heard.recv() {
            taken.fetch_add(1, Ordering::SeqCst);
            match next {
                Heard::Said(message) => {
                    assert_eq!(message, cancel);
                    said += 1;
                }
                Heard::Ended => break,
                other => panic!("{other:?}"),
            }
        }
        assert!(reader.join().is_ok(), "the reader read too far ahead");
        assert_eq!(said, FRAMES);
    }
}
