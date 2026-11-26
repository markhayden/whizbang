//! The supervisor of the tool worker (plan 2C, task X1, items 2 and 3; ADR 0032): the
//! second process of the pack's runtime, which runs a user's tool file, and its protocol,
//! `tool/1`, beside `pack/1`, which is unchanged (ADR 0020: a new kind of request is a new
//! version).
//!
//! **What it is handed** is what the model's worker is ([`super::worker`]): the pack's
//! Python by its absolute path, isolated and writing no bytecode, with the boot that closes
//! every descriptor above the three; three pipes; and a cleared environment, given the
//! locale and the supervisor's own variables and no key, no token and no home of the
//! user's ([`super::worker::environment`]): an empty folder of its own, removed once it
//! was waited for. It is not kept from the network or the disk: a tool file is code the
//! user chose and runs with the user's privileges, as a user's adapter does (ADR 0032).
//!
//! **It leads a process group of its own** (`process_group(0)`, set before its program
//! starts), whose id is its process id, recorded when it is started. Every stop closes its
//! input, and the worker, at the end of its input, ends every process of its group with
//! itself (`pack/src/whizbang_pack/tool_worker.py`, `end_group`): a process a tool started
//! is ended with it, one holding its pipes among them, and the readers here end with the
//! last holder. A worker that has not ended [`END_LIMIT`] after its input closed cannot
//! act (a tool holding Python's lock in C code, say): whizbang then kills that group
//! itself, with the system's `kill` program (`/bin/kill -KILL -- -<group>`, [`kill_group`]),
//! waits for the program, and kills and waits for the worker, on that path of
//! [`ToolWorker::finish`] and of the drop of a running worker. Only the group whizbang made
//! for that worker is named: never 0 or 1, never this process's own. The owner approved
//! whizbang signalling its tool worker's group so on 2026-11-18; the workspace forbids the
//! `unsafe` a signal would take in Rust. A process a tool moved out of the group is not
//! ended; its readers are let go, and no worker is started again while they live.
//!
//! **One worker for one file.** A run of a file of another digest than the worker's last
//! ends that worker and starts a new one, so that nothing one file's code left in the
//! process (a module, a thread, a global) is there for another, or for an edit of it.
//!
//! **A run** carries the tool file's bytes, their digest, the run's arguments and the
//! images the tool declared, each its samples. Its end is the tool's output, or a fault;
//! every fault but a refusal before a byte was written stops the worker: a tool's own
//! failure, a cancellation, the run's time, a frame that breaks `tool/1`. The output is
//! JSON within the header's bound; what it means is held to the tool's declaration by the
//! caller before anything uses it.
//!
//! **A capability a tool asks for** comes as an `ask` and is answered by the caller's
//! `answer`, which makes it a nested call of the tool's run (ADR 0008), under the run's
//! allowance and policy. Two capabilities: `segment`, one query of a picture, answered
//! with masks; and `labels` (plan mE, task T2; ADR 0052), the classes a fixed-class
//! segmenter labels a picture's pixels with, answered with one plane of each pixel's class
//! and each class's confidence. Either may name a model of the catalogue, which the caller
//! routes to. One is open at a time: an `ask` heard while another is being
//! answered is refused, and a run asks at most [`ASKS_LIMIT`] times. The worker's frames
//! carry no payload: a header of at most [`HEADER_LIMIT`] bytes is all whizbang reads.

use std::io::{self, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::sync::{Arc, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::{Map, Value as Json, json};
use whizbang_core::contract::{Cancellation, Clock, Secrets};

use super::threads::{self, Spawn};
use super::worker::{BOOT, END_LIMIT, Ended, Kept, PACE, environment, kill, make_home};

/// The one version this side speaks.
pub const VERSION: &str = "tool/1";
/// The entry point of the tool worker.
pub const TOOL_MODULE: &str = "whizbang_pack.tool_worker";
/// The most bytes of a header either side writes.
pub const HEADER_LIMIT: usize = 64 * 1024;
/// The most bytes of a tool file.
pub const CODE_LIMIT: usize = 1 << 20;
/// The most images one run hands a tool.
pub const IMAGES_LIMIT: usize = 8;
/// The most capabilities one run asks for.
pub const ASKS_LIMIT: u64 = 64;
/// The most instances a tool asks one query for.
pub const MOST_LIMIT: u64 = 64;
/// The most characters of a phrase a tool asks for, and of a class.
pub const PHRASE_LIMIT: usize = 512;
/// The most classes a tool asks labels of: what one byte of a pixel tells apart, beside
/// none.
pub const CLASSES_LIMIT: usize = 255;
/// The most characters of a model's name an ask gives.
pub const MODEL_LIMIT: usize = 64;
/// The most characters of a sentence a worker's `error` gives that are kept.
pub const SENTENCE_LIMIT: usize = 240;
/// How long a tool worker has to start and say hello.
pub const HELLO_LIMIT: Duration = Duration::from_secs(60);
/// The longest one run may take, whatever its declaration says.
pub const RUN_LIMIT: Duration = Duration::from_secs(600);
/// How many of the worker's frames may wait unread.
const FRAMES_WAITING: usize = 8;

/// What starts the tool worker: the interpreter by its absolute path, the pack's source,
/// the entry point and its lifetime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolLaunch {
    pub python: PathBuf,
    pub source: PathBuf,
    module: String,
    pub lifetime_s: u32,
}

impl ToolLaunch {
    /// The tool worker, as the kind `pack` starts it.
    pub fn worker(python: PathBuf, source: PathBuf, lifetime_s: u32) -> ToolLaunch {
        ToolLaunch {
            python,
            source,
            module: TOOL_MODULE.to_owned(),
            lifetime_s,
        }
    }
}

/// An image a run hands its tool: the name the tool declared it under, its size, and its
/// samples, red, green, blue and alpha for each pixel, row by row.
#[derive(Clone, PartialEq, Eq)]
pub struct ToolImage {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// What a tool asked: one query of `segment`, or the classes of `labels`.
#[derive(Debug, Clone, PartialEq)]
pub enum Sought {
    Phrase(String),
    Point([f64; 2]),
    Box([f64; 4]),
    /// The pixels a fixed-class segmenter labels with each of these classes, 1 to
    /// [`CLASSES_LIMIT`], each a phrase.
    Labels(Vec<String>),
}

/// A capability a tool asked for, on the image of this name: `segment`, one query, at
/// most `most` instances; or `labels`, its classes, `most` then 1. `model` is the model of
/// the catalogue the tool named, when it named one: a name of 1 to [`MODEL_LIMIT`]
/// lower-case letters, digits, `-` and `_`, starting with a letter.
#[derive(Debug, Clone, PartialEq)]
pub struct Asked {
    pub image: String,
    pub sought: Sought,
    pub most: u32,
    pub model: Option<String>,
}

/// What answers a `labels` ask: the image's size, the classes some pixel has with each
/// one's confidence, and one byte for each pixel, row by row: 0 where no class asked
/// labels it, and `k` where the `k`th of `classes` does.
#[derive(Clone, PartialEq)]
pub struct Labelled {
    pub width: u32,
    pub height: u32,
    pub classes: Vec<(String, f64)>,
    pub plane: Vec<u8>,
}

/// What the caller answers an ask with: masks, for `segment`; a picture's labels, for
/// `labels`.
#[derive(Clone, PartialEq)]
pub enum Answered {
    Masks(Vec<Mask>),
    Labels(Labelled),
}

impl From<Vec<Mask>> for Answered {
    fn from(masks: Vec<Mask>) -> Answered {
        Answered::Masks(masks)
    }
}

/// One instance that answers an ask: its score, its box in pixels, and its mask on the
/// image's own size, one byte for each pixel, 255 where it is.
#[derive(Clone, PartialEq)]
pub struct Mask {
    /// The segmenter's score, when the answer has one: a mask in the store holds none.
    pub score: Option<f64>,
    pub bounds: [f64; 4],
    pub width: u32,
    pub height: u32,
    pub plane: Vec<u8>,
}

/// One run of a tool: its name, the digest of its file as 64 hex digits, the file's
/// bytes, its arguments, and its images.
pub struct ToolRun {
    pub tool: String,
    pub digest: String,
    pub code: Vec<u8>,
    pub arguments: Map<String, Json>,
    pub images: Vec<ToolImage>,
}

/// The ends of a run's wait: the run's cancellation, its deadline, and its limit.
pub struct ToolLimits<'a> {
    pub cancellation: &'a Cancellation,
    pub deadline: Option<Instant>,
    pub limit: Duration,
}

/// Why a run gave no output, in fixed words and typed fields; the sentence of a tool's
/// own failure is the worker's, cleaned and cut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolFault {
    /// Refused before a byte of it was written; the worker lives on.
    NotSent(String),
    /// The tool failed and said why.
    Failed(String),
    Cancelled,
    TimedOut,
    /// The worker could not be started, ended, or broke `tool/1`: why, in fixed words.
    Unavailable(String),
}

impl std::fmt::Display for ToolFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolFault::NotSent(why) => write!(f, "the run was not sent to the tool worker: {why}"),
            ToolFault::Failed(said) => write!(f, "the tool failed: {said}"),
            ToolFault::Cancelled => f.write_str("the run was cancelled, and the tool worker ended"),
            ToolFault::TimedOut => {
                f.write_str("the run passed its time, and the tool worker was ended")
            }
            ToolFault::Unavailable(why) => write!(f, "the tool worker {why}"),
        }
    }
}

/// What the reader of the worker's output hands on.
enum Heard {
    Frame(Map<String, Json>),
    Ended,
    Refused(&'static str),
}

/// A worker that is running, from the moment its process is started. Its drop ends it.
struct Running {
    child: Child,
    reaped: Option<Ended>,
    input: Option<SyncSender<Vec<Vec<u8>>>>,
    heard: Receiver<Heard>,
    readers: Vec<JoinHandle<()>>,
    home: Option<PathBuf>,
    /// The process group whizbang made for the worker: its process id.
    group: u32,
    /// The digest of the tool file it was started for.
    digest: String,
}

impl Drop for Running {
    /// The end of a worker on a path that did not reach [`ToolWorker::finish`], a panic
    /// among them: its input closed, so that it ends its group; killed past
    /// [`END_LIMIT`] of the machine's time; waited for; its folder removed.
    fn drop(&mut self) {
        if self.reaped.is_none() {
            drop(self.input.take());
            let until = Instant::now() + END_LIMIT;
            while matches!(self.child.try_wait(), Ok(None)) && Instant::now() < until {
                std::thread::sleep(PACE);
            }
            if matches!(self.child.try_wait(), Ok(None)) {
                kill_group(self.group);
            }
            kill(&mut self.child);
        }
        if let Some(home) = self.home.take() {
            let _ = std::fs::remove_dir_all(home);
        }
    }
}

/// The supervisor of the one tool worker. It starts none until a run asks.
pub struct ToolWorker {
    launch: ToolLaunch,
    clock: Arc<dyn Clock>,
    secrets: Secrets,
    keys: Vec<String>,
    parent: Vec<(std::ffi::OsString, std::ffi::OsString)>,
    spawn: Spawn,
    running: Option<Running>,
    kept: Kept,
    runs: u64,
    /// The readers of a worker that ended while something held its pipes: no worker is
    /// started while one lives.
    lingering: Vec<JoinHandle<()>>,
    ended: Option<Ended>,
    asks: Arc<AtomicU64>,
}

impl ToolWorker {
    pub fn new(launch: ToolLaunch, clock: Arc<dyn Clock>, secrets: Secrets) -> ToolWorker {
        ToolWorker {
            launch,
            clock,
            secrets,
            keys: Vec::new(),
            parent: std::env::vars_os().collect(),
            spawn: threads::spawn,
            running: None,
            kept: Arc::default(),
            runs: 0,
            lingering: Vec::new(),
            ended: None,
            asks: Arc::default(),
        }
    }

    /// The same, told the names of the config's keys, none of which the worker is given.
    pub fn with_keys(mut self, keys: impl IntoIterator<Item = String>) -> ToolWorker {
        self.keys = keys.into_iter().collect();
        self
    }

    /// The same, choosing the worker's environment from these in place of this process's.
    pub fn with_parent_environment(
        mut self,
        parent: impl IntoIterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
    ) -> ToolWorker {
        self.parent = parent.into_iter().collect();
        self
    }

    /// The same, making its three threads with this: a test's.
    pub fn with_spawn(mut self, spawn: Spawn) -> ToolWorker {
        self.spawn = spawn;
        self
    }

    /// The process id of the worker that runs, if one does.
    pub fn pid(&self) -> Option<u32> {
        self.running.as_ref().map(|running| running.child.id())
    }

    /// How the last worker ended.
    pub fn ended(&self) -> Option<Ended> {
        self.ended
    }

    /// The last lines of the worker's error output, redacted and cut.
    pub fn error_output(&self) -> Vec<String> {
        let kept = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
        kept.iter().cloned().collect()
    }

    /// How many asks the readers of this supervisor's workers have read, counted as
    /// they read them: for a test that waits until a second is there.
    pub fn asks_heard(&self) -> Arc<AtomicU64> {
        self.asks.clone()
    }

    /// Ends the worker that runs, and its group: its input closed, [`END_LIMIT`] for it
    /// to end, then killed; waited for either way. Says how it ended.
    pub fn stop(&mut self) -> Option<Ended> {
        let running = self.running.take()?;
        let ended = self.finish(running);
        self.ended = Some(ended);
        Some(ended)
    }

    fn finish(&mut self, mut running: Running) -> Ended {
        drop(running.input.take());
        let clock = self.clock.clone();
        let ended = until(&*clock, END_LIMIT, || {
            running.child.try_wait().ok().flatten().map(Ended::of)
        })
        .unwrap_or_else(|| {
            // The worker could not end its group: whizbang ends it, then the worker.
            kill_group(running.group);
            kill(&mut running.child)
        });
        running.reaped = Some(ended);
        until(&*clock, END_LIMIT, || {
            running
                .readers
                .iter()
                .all(JoinHandle::is_finished)
                .then_some(())
        });
        for reader in running.readers.drain(..) {
            if reader.is_finished() {
                let _ = reader.join();
            } else {
                self.lingering.push(reader);
            }
        }
        if let Some(home) = running.home.take() {
            let _ = std::fs::remove_dir_all(home);
        }
        ended
    }

    /// Starts the worker unless one runs, and waits for its hello.
    fn ready(&mut self, digest: &str) -> Result<(), ToolFault> {
        if let Some(running) = &mut self.running {
            if running.digest == digest && matches!(running.child.try_wait(), Ok(None)) {
                return Ok(());
            }
            // Ended, or started for another file: a file is run by a worker of its own.
            self.stop();
        }
        self.lingering.retain(|reader| !reader.is_finished());
        if !self.lingering.is_empty() {
            return Err(ToolFault::NotSent(
                "a process a tool started still holds the last tool worker's pipes; no worker is started until it ends".to_owned(),
            ));
        }
        let launch = &self.launch;
        if !launch.python.is_absolute() {
            return Err(ToolFault::NotSent(
                "the pack's Python is not an absolute path; run whizbang setup".to_owned(),
            ));
        }
        let home = make_home().map_err(|_| {
            ToolFault::NotSent("the tool worker could not be given a folder".to_owned())
        })?;
        let mut command = Command::new(&launch.python);
        command
            .args(["-I", "-B", "-c", BOOT])
            .arg(&launch.source)
            .arg(&launch.module)
            .arg("--max-lifetime-s")
            .arg(launch.lifetime_s.max(1).to_string())
            .env_clear()
            .envs(environment(&self.parent, &self.keys, &home))
            .current_dir(&home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(_) => {
                let _ = std::fs::remove_dir_all(&home);
                return Err(ToolFault::NotSent(
                    "the tool worker could not be started".to_owned(),
                ));
            }
        };
        let group = child.id();
        let streams = (child.stdin.take(), child.stdout.take(), child.stderr.take());
        let (tell, heard) = mpsc::sync_channel(FRAMES_WAITING);
        let (jobs, queued) = mpsc::sync_channel::<Vec<Vec<u8>>>(4);
        let asks = self.asks.clone();
        self.kept = Arc::default();
        let mut running = Running {
            child,
            reaped: None,
            input: Some(jobs),
            heard,
            readers: Vec::new(),
            home: Some(home),
            group,
            digest: String::new(),
        };
        let (Some(input), Some(output), Some(errors)) = streams else {
            self.finish(running);
            return Err(ToolFault::NotSent(
                "the tool worker was started without its pipes".to_owned(),
            ));
        };
        let (kept, secrets) = (self.kept.clone(), self.secrets.clone());
        let made = [
            (self.spawn)(
                "whizbang-tool-output".to_owned(),
                Box::new(move || read_frames(output, &tell, &asks)),
            ),
            (self.spawn)(
                "whizbang-tool-errors".to_owned(),
                Box::new(move || super::worker::read_error_output(errors, &kept, &secrets)),
            ),
            (self.spawn)(
                "whizbang-tool-input".to_owned(),
                Box::new(move || write_frames(input, &queued)),
            ),
        ];
        let mut all = true;
        for thread in made {
            match thread {
                Ok(thread) => running.readers.push(thread),
                Err(_) => all = false,
            }
        }
        if !all {
            self.ended = Some(self.finish(running));
            return Err(ToolFault::NotSent(
                "the threads of the tool worker could not be made".to_owned(),
            ));
        }
        let hello = json!({"type": "hello", "version": VERSION, "role": "whizbang"});
        let said = send(&running, &hello, Vec::new())
            .map_err(str::to_owned)
            .and_then(|()| {
                let clock = self.clock.clone();
                match until(&*clock, HELLO_LIMIT, || next(&running)) {
                    Some(Heard::Frame(frame))
                        if frame.len() == 3
                            && frame.get("type") == Some(&json!("hello"))
                            && frame.get("version") == Some(&json!(VERSION))
                            && frame.get("role") == Some(&json!("worker")) =>
                    {
                        Ok(())
                    }
                    Some(_) => Err("answered no hello of tool/1".to_owned()),
                    None => Err(format!(
                        "said no hello within {} seconds",
                        HELLO_LIMIT.as_secs()
                    )),
                }
            });
        match said {
            Ok(()) => {
                self.running = Some(running);
                Ok(())
            }
            Err(why) => {
                self.ended = Some(self.finish(running));
                Err(ToolFault::Unavailable(why))
            }
        }
    }

    /// Runs a tool: its output, or why there is none. Each ask is handed to `answer`,
    /// whose masks or labels, or whose refusal, go back to the tool.
    pub fn run(
        &mut self,
        run: ToolRun,
        limits: &ToolLimits<'_>,
        answer: &mut dyn FnMut(&Asked) -> Result<Answered, String>,
    ) -> Result<Json, ToolFault> {
        let ended_before = |clock: &dyn Clock| {
            if limits.cancellation.is_cancelled() {
                Some(ToolFault::Cancelled)
            } else if limits.deadline.is_some_and(|by| clock.now() >= by) {
                Some(ToolFault::TimedOut)
            } else {
                None
            }
        };
        let (header, payloads) = run_frame(&run, self.runs + 1)?;
        if let Some(fault) = ended_before(&*self.clock) {
            return Err(fault);
        }
        self.ready(&run.digest)?;
        if let Some(running) = &mut self.running {
            running.digest = run.digest.clone();
        }
        if let Some(fault) = ended_before(&*self.clock) {
            return Err(fault);
        }
        self.runs += 1;
        let id = format!("r{}", self.runs);
        let names: Vec<String> = run.images.iter().map(|image| image.name.clone()).collect();
        let Some(running) = self.running.take() else {
            return Err(ToolFault::Unavailable("is not running".to_owned()));
        };
        let ended = self.converse(&running, &id, header, payloads, &names, limits, answer);
        match ended {
            Ok(output) => {
                self.running = Some(running);
                Ok(output)
            }
            Err(fault) => {
                self.ended = Some(self.finish(running));
                Err(fault)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn converse(
        &self,
        running: &Running,
        id: &str,
        header: Json,
        payloads: Vec<Vec<u8>>,
        names: &[String],
        limits: &ToolLimits<'_>,
        answer: &mut dyn FnMut(&Asked) -> Result<Answered, String>,
    ) -> Result<Json, ToolFault> {
        let broke = |why: &str| ToolFault::Unavailable(format!("broke tool/1: {why}"));
        send(running, &header, payloads).map_err(|why| ToolFault::Unavailable(why.to_owned()))?;
        let limit = limits.limit.min(RUN_LIMIT);
        let begun = self.clock.now();
        let end = match limits.deadline {
            Some(deadline) => deadline.min(begun + limit),
            None => begun + limit,
        };
        let mut pending: std::collections::VecDeque<Heard> = std::collections::VecDeque::new();
        let mut last_ask = 0u64;
        let most = 2 * (limit.as_millis() / PACE.as_millis()).max(1);
        for _ in 0..=most {
            let heard = match pending.pop_front() {
                Some(heard) => Some(heard),
                None => next(running),
            };
            let Some(heard) = heard else {
                if limits.cancellation.is_cancelled() {
                    return Err(ToolFault::Cancelled);
                }
                if self.clock.now() >= end {
                    return Err(ToolFault::TimedOut);
                }
                self.clock.pause(PACE);
                continue;
            };
            let frame = match heard {
                Heard::Frame(frame) => frame,
                Heard::Ended => {
                    return Err(ToolFault::Unavailable("ended inside the run".to_owned()));
                }
                Heard::Refused(why) => return Err(broke(why)),
            };
            if frame.get("id") != Some(&json!(id)) {
                return Err(broke("a frame names another run"));
            }
            match frame.get("type").and_then(Json::as_str) {
                Some("result") => {
                    exactly(&frame, &["type", "id", "output"]).map_err(broke)?;
                    return match frame.get("output") {
                        Some(output @ Json::Object(_)) => Ok(output.clone()),
                        _ => Err(broke("a result's output is no object")),
                    };
                }
                Some("error") => {
                    exactly(&frame, &["type", "id", "sentence"]).map_err(broke)?;
                    let said = frame.get("sentence").and_then(Json::as_str).unwrap_or("");
                    return Err(ToolFault::Failed(clean(said, SENTENCE_LIMIT)));
                }
                Some("ask") => {
                    let (number, asked) = asked_of(&frame, names).map_err(broke)?;
                    if number <= last_ask || number > ASKS_LIMIT {
                        return Err(broke("an ask's number is not the next"));
                    }
                    last_ask = number;
                    let answered = answer(&asked);
                    // An ask heard while this one was open is refused, and what else
                    // came meanwhile is taken next: at most [`FRAMES_WAITING`] of it,
                    // looked at for at most twice that many frames, within the run's time.
                    for _ in 0..2 * FRAMES_WAITING {
                        if self.clock.now() >= end {
                            return Err(ToolFault::TimedOut);
                        }
                        let Some(more) = next(running) else {
                            break;
                        };
                        match &more {
                            Heard::Frame(other)
                                if other.get("type") == Some(&json!("ask"))
                                    && other.get("id") == Some(&json!(id)) =>
                            {
                                let Some(other_number) = other.get("ask").and_then(Json::as_u64)
                                else {
                                    return Err(broke("an ask has no number"));
                                };
                                if other_number <= last_ask || other_number > ASKS_LIMIT {
                                    return Err(broke("an ask's number is not the next"));
                                }
                                last_ask = other_number;
                                let refused = json!({"type": "refused", "id": id, "ask": other_number,
                                    "sentence": "one capability request is open at a time; this one came while another was"});
                                send(running, &refused, Vec::new())
                                    .map_err(|why| ToolFault::Unavailable(why.to_owned()))?;
                            }
                            _ if pending.len() >= FRAMES_WAITING => {
                                return Err(broke(
                                    "more frames came while an ask was open than are kept",
                                ));
                            }
                            _ => pending.push_back(more),
                        }
                    }
                    let (header, payloads) = answer_frame(id, number, answered);
                    send(running, &header, payloads)
                        .map_err(|why| ToolFault::Unavailable(why.to_owned()))?;
                }
                _ => return Err(broke("a frame this side does not take")),
            }
        }
        Err(ToolFault::TimedOut)
    }
}

impl Drop for ToolWorker {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The system's `kill` program, by its absolute path: never looked for on `PATH`.
const KILL: &str = "/bin/kill";

/// Ends every process of the group whizbang made for a tool worker, by the system's
/// `kill` program (`/bin/kill -KILL -- -<group>`), and waits for the program, at most
/// [`END_LIMIT`]. The owner approved it on 2026-11-18 (ADR 0032, "Nothing left
/// running"). `group` is the worker's process id, which `process_group(0)` made the id of
/// a new group of its own: never 0 or 1, and never this process's group, which is not one
/// a child of it was made the leader of. A group that is none of these is not signalled.
fn kill_group(group: u32) {
    let named = group > 1 && group != std::process::id();
    debug_assert!(named, "a tool worker's group is never 0, 1 or this process");
    if !named {
        return;
    }
    let started = Command::new(KILL)
        .arg("-KILL")
        .arg("--")
        .arg(format!("-{group}"))
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut killing) = started else {
        return;
    };
    let until = Instant::now() + END_LIMIT;
    while matches!(killing.try_wait(), Ok(None)) && Instant::now() < until {
        std::thread::sleep(PACE);
    }
    kill(&mut killing);
}

/// Waits for `look` to give something, looking every [`PACE`] of the clock, until `limit`
/// of it has passed; and at most a bounded count of looks, so a clock that stands still
/// ends the wait too.
fn until<T>(clock: &dyn Clock, limit: Duration, mut look: impl FnMut() -> Option<T>) -> Option<T> {
    let end = clock.now() + limit;
    let most = 2 * (limit.as_millis() / PACE.as_millis()).max(1);
    for _ in 0..=most {
        if let Some(found) = look() {
            return Some(found);
        }
        if clock.now() >= end {
            return None;
        }
        clock.pause(PACE);
    }
    None
}

/// What the reader handed on, if anything is there.
fn next(running: &Running) -> Option<Heard> {
    match running.heard.try_recv() {
        Ok(heard) => Some(heard),
        Err(TryRecvError::Empty) => None,
        Err(TryRecvError::Disconnected) => Some(Heard::Ended),
    }
}

/// Hands a frame to the writer, waiting for nothing: refused when the writer has ended or
/// is behind.
fn send(running: &Running, header: &Json, payloads: Vec<Vec<u8>>) -> Result<(), &'static str> {
    let mut header = header.clone();
    if !payloads.is_empty()
        && let Json::Object(fields) = &mut header
    {
        let sizes: Vec<Json> = payloads.iter().map(|p| json!(p.len())).collect();
        fields.insert("payloads".to_owned(), Json::Array(sizes));
    }
    let bytes = serde_json::to_vec(&header).map_err(|_| "could not be written a frame")?;
    if bytes.is_empty() || bytes.len() > HEADER_LIMIT {
        return Err("would be sent a header over its bound");
    }
    let mut chunks = Vec::with_capacity(2 + 2 * payloads.len());
    chunks.push((bytes.len() as u32).to_be_bytes().to_vec());
    chunks.push(bytes);
    for payload in payloads {
        chunks.push((payload.len() as u64).to_be_bytes().to_vec());
        chunks.push(payload);
    }
    let Some(input) = &running.input else {
        return Err("has no input");
    };
    input
        .try_send(chunks)
        .map_err(|_| "does not take its input")
}

/// The header and payloads of a run: refused before anything is sent when it breaks a
/// bound.
fn run_frame(run: &ToolRun, number: u64) -> Result<(Json, Vec<Vec<u8>>), ToolFault> {
    let refuse = |why: &str| Err(ToolFault::NotSent(why.to_owned()));
    if run.code.is_empty() || run.code.len() > CODE_LIMIT {
        return refuse("a tool file is 1 byte to 1 MiB");
    }
    if run.images.len() > IMAGES_LIMIT {
        return refuse("a run hands its tool at most 8 images");
    }
    let mut images = Vec::new();
    let mut payloads = vec![run.code.clone()];
    for image in &run.images {
        let size = 4 * u64::from(image.width) * u64::from(image.height);
        if image.rgba.len() as u64 != size || size == 0 {
            return refuse("an image's samples are not its size");
        }
        images.push(json!({"name": image.name, "width": image.width, "height": image.height}));
        payloads.push(image.rgba.clone());
    }
    let header = json!({
        "type": "run",
        "id": format!("r{number}"),
        "tool": {"name": run.tool, "digest": run.digest},
        "arguments": Json::Object(run.arguments.clone()),
        "images": images,
    });
    Ok((header, payloads))
}

/// The answer to an ask: its masks, its labels, or its refusal.
fn answer_frame(id: &str, number: u64, answered: Result<Answered, String>) -> (Json, Vec<Vec<u8>>) {
    let refused = |why: &str| {
        let sentence = clean(why, SENTENCE_LIMIT);
        (
            json!({"type": "refused", "id": id, "ask": number, "sentence": sentence}),
            Vec::new(),
        )
    };
    let masks = match answered {
        Ok(Answered::Masks(masks)) => masks,
        Ok(Answered::Labels(labelled)) => {
            let fits = labelled.plane.len() as u64
                == u64::from(labelled.width) * u64::from(labelled.height)
                && labelled.classes.len() <= CLASSES_LIMIT
                && (labelled.plane.iter()).all(|k| usize::from(*k) <= labelled.classes.len())
                && labelled
                    .classes
                    .iter()
                    .all(|(_, score)| (0.0..=1.0).contains(score));
            if !fits {
                return refused("the labels found are not of one image's size and its classes");
            }
            let classes: Vec<Json> = (labelled.classes.iter())
                .map(|(name, score)| json!({"name": name, "score": score}))
                .collect();
            let header = json!({"type": "answer", "id": id, "ask": number,
                "width": labelled.width, "height": labelled.height, "labels": classes});
            return (header, vec![labelled.plane]);
        }
        Err(why) => return refused(&why),
    };
    let (width, height) = masks.first().map_or((0, 0), |m| (m.width, m.height));
    let fits = masks.iter().all(|m| {
        (m.width, m.height) == (width, height)
            && m.plane.len() as u64 == u64::from(width) * u64::from(height)
    });
    if !fits || masks.len() as u64 > MOST_LIMIT {
        return refused("the masks found are not of one image's size");
    }
    let instances: Vec<Json> = masks
        .iter()
        .map(|m| json!({"score": m.score, "box": m.bounds}))
        .collect();
    let header = json!({"type": "answer", "id": id, "ask": number, "width": width,
        "height": height, "instances": instances});
    (header, masks.into_iter().map(|m| m.plane).collect())
}

/// Holds a frame to exactly these fields.
fn exactly(frame: &Map<String, Json>, fields: &[&str]) -> Result<(), &'static str> {
    if frame.len() == fields.len() && fields.iter().all(|f| frame.contains_key(*f)) {
        Ok(())
    } else {
        Err("a frame lacks a field of its own, or holds one that is not")
    }
}

/// An ask, held to `tool/1`: its number, and what it asks. A `segment` holds its `query`
/// and its `most`; a `labels` its `classes`; either may hold `model`.
fn asked_of(frame: &Map<String, Json>, names: &[String]) -> Result<(u64, Asked), &'static str> {
    let labels = match frame.get("capability").and_then(Json::as_str) {
        Some("segment") => false,
        Some("labels") => true,
        _ => return Err("an ask is of a capability a tool cannot ask for"),
    };
    let mut fields = vec!["type", "id", "ask", "capability", "image"];
    fields.extend(if labels {
        &["classes"][..]
    } else {
        &["query", "most"][..]
    });
    if frame.contains_key("model") {
        fields.push("model");
    }
    exactly(frame, &fields)?;
    let number = frame
        .get("ask")
        .and_then(Json::as_u64)
        .ok_or("an ask has no number")?;
    let model = match frame.get("model") {
        None => None,
        Some(model) => {
            let model = model.as_str().ok_or("an ask's model is no text")?;
            let named = model.len() <= MODEL_LIMIT
                && model.bytes().next().is_some_and(|b| b.is_ascii_lowercase())
                && (model.bytes()).all(|b| {
                    b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_'
                });
            if !named {
                return Err("an ask's model is not a model's name");
            }
            Some(model.to_owned())
        }
    };
    let image = frame
        .get("image")
        .and_then(Json::as_str)
        .ok_or("an ask names no image")?;
    if !names.iter().any(|name| name == image) {
        return Err("an ask names an image the run did not hand its tool");
    }
    if labels {
        let listed = (frame.get("classes").and_then(Json::as_array))
            .ok_or("an ask's classes are no list")?;
        if listed.is_empty() || listed.len() > CLASSES_LIMIT {
            return Err("an ask's classes are 1 to 255");
        }
        let mut classes: Vec<String> = Vec::new();
        for class in listed {
            let text = class.as_str().ok_or("a class is no text")?;
            if text.is_empty() || text.chars().count() > PHRASE_LIMIT || text.contains('\n') {
                return Err("a class is 1 to 512 characters of one line");
            }
            if classes.iter().any(|given| given == text) {
                return Err("a class is asked twice");
            }
            classes.push(text.to_owned());
        }
        let asked = Asked {
            image: image.to_owned(),
            sought: Sought::Labels(classes),
            most: 1,
            model,
        };
        return Ok((number, asked));
    }
    let most = frame
        .get("most")
        .and_then(Json::as_u64)
        .ok_or("an ask gives no most")?;
    if !(1..=MOST_LIMIT).contains(&most) {
        return Err("an ask's most is not from 1 to 64");
    }
    let Some(Json::Object(query)) = frame.get("query") else {
        return Err("an ask's query is no object");
    };
    let numbers = |value: &Json, count: usize| -> Option<Vec<f64>> {
        let list = value.as_array()?;
        let read: Option<Vec<f64>> = list.iter().map(Json::as_f64).collect();
        read.filter(|read| read.len() == count && read.iter().all(|n| n.is_finite()))
    };
    let sought = match query.iter().next() {
        Some((kind, value)) if query.len() == 1 => match kind.as_str() {
            "phrase" => {
                let text = value.as_str().ok_or("a phrase is no text")?;
                if text.is_empty() || text.chars().count() > PHRASE_LIMIT {
                    return Err("a phrase is 1 to 512 characters");
                }
                Sought::Phrase(text.to_owned())
            }
            "point" => {
                let n = numbers(value, 2).ok_or("a point is two numbers")?;
                Sought::Point([n[0], n[1]])
            }
            "box" => {
                let n = numbers(value, 4).ok_or("a box is four numbers")?;
                Sought::Box([n[0], n[1], n[2], n[3]])
            }
            _ => return Err("a query is a phrase, a point or a box"),
        },
        _ => return Err("a query is one of a phrase, a point and a box"),
    };
    let asked = Asked {
        image: image.to_owned(),
        sought,
        most: most as u32,
        model,
    };
    Ok((number, asked))
}

/// A text of printable ASCII, cut: every other character a space.
fn clean(text: &str, most: usize) -> String {
    let kept: String = text
        .chars()
        .map(|c| if (' '..='~').contains(&c) { c } else { ' ' })
        .collect();
    kept.trim().chars().take(most).collect()
}

/// The thread that reads the worker's output: each frame's header, handed on, until the
/// output ends, a frame is refused, or nobody listens. Asks are counted as they are read.
fn read_frames(output: impl Read, tell: &SyncSender<Heard>, asks: &AtomicU64) {
    let mut output = BufReader::new(output);
    loop {
        let heard = match read_frame(&mut output) {
            Ok(Some(frame)) => {
                if frame.get("type") == Some(&json!("ask")) {
                    asks.fetch_add(1, Ordering::SeqCst);
                }
                Heard::Frame(frame)
            }
            Ok(None) => Heard::Ended,
            Err(why) => Heard::Refused(why),
        };
        let last = !matches!(heard, Heard::Frame(_));
        if tell.send(heard).is_err() || last {
            // Whatever is left is read and dropped, so that a worker that writes on
            // never blocks on a full pipe.
            let _ = io::copy(&mut output, &mut io::sink());
            return;
        }
    }
}

/// One frame of the worker's: a header within its bound, an object with a type, and no
/// payload. `None` at the end of the output before a frame's first byte.
fn read_frame(input: &mut impl Read) -> Result<Option<Map<String, Json>>, &'static str> {
    let mut length = [0u8; 4];
    let mut got = 0;
    while got < 4 {
        match input.read(&mut length[got..]) {
            Ok(0) if got == 0 => return Ok(None),
            Ok(0) => return Err("the output ended inside a frame"),
            Ok(n) => got += n,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return Err("the output could not be read"),
        }
    }
    let size = u32::from_be_bytes(length) as usize;
    if size == 0 || size > HEADER_LIMIT {
        return Err("a header's length is 0 or over its bound");
    }
    let mut header = vec![0u8; size];
    input
        .read_exact(&mut header)
        .map_err(|_| "the output ended inside a frame")?;
    let Ok(Json::Object(frame)) = serde_json::from_slice::<Json>(&header) else {
        return Err("a header is not one JSON object");
    };
    if !frame.get("type").is_some_and(Json::is_string) {
        return Err("a header has no type");
    }
    if frame.contains_key("payloads") {
        return Err("a frame of the worker's carries no payload");
    }
    Ok(Some(frame))
}

/// The thread that writes the worker's input: each frame, in order, until the frames stop
/// or a write fails. Its end closes the input.
fn write_frames(mut input: ChildStdin, frames: &Receiver<Vec<Vec<u8>>>) {
    for chunks in frames.iter() {
        for chunk in &chunks {
            if input.write_all(chunk).is_err() {
                return;
            }
        }
        if input.flush().is_err() {
            return;
        }
    }
}
