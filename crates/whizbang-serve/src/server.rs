//! The loop of the server (ADR 0031). One thread reads the input ([`crate::frame`]) and
//! hands each message over a channel of [`QUEUED_LIMIT`]; this loop takes them, starts a
//! run for each call of a tool through the executor's entry point
//! ([`Executor::start`]), and between messages looks at each run it started: the events
//! the run has published go to the client as progress, a run that ended is answered,
//! and a run still going at its stated time ([`Limits::wait`]) is cancelled and the
//! call answered that the server gave up on it. At the end of the input, every run still
//! going is cancelled and waited for, up to [`Limits::grace`], and then the door is told
//! to stop what it holds (the pack's worker).
//!
//! **Only the protocol is written to the output**, one message to a line. Nothing a
//! client sent is said back, beside the id of its request and the token it asked
//! progress under, which the protocol returns. What the executor says of a run has had
//! the secrets taken out already; every text the server sends has its control
//! characters escaped too ([`escaped`]), so no event or reason reaches a terminal raw.

use std::io::{self, BufReader, Read, Write};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, sync_channel};
use std::time::{Duration, Instant};

use serde_json::{Map, Value as Json, json};
use whizbang_core::contract::{
    Answer, Call, Cancellation, Executor, JournalLine, Outcome, Request, RunId, RunState,
};

use crate::frame::{self, Frame};
pub use crate::tools::Tool;

/// The revisions of the protocol the server speaks, newest first. A client that asks
/// for another is answered with the newest.
pub const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];
/// The most calls that run at once; one more is answered that the server is busy.
pub const CALLS_LIMIT: usize = 4;
/// The most messages read and not yet taken: the reader waits for the loop past it.
const QUEUED_LIMIT: usize = 8;
/// The most characters of a request's id, or of a progress token, the server takes.
const ID_LIMIT: usize = 128;

/// What a door tells the client of a run that ended: the text, and whether the call
/// failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Said {
    pub text: String,
    pub error: bool,
}

/// What the binary hands the server beside the executor: the tools it offers, how a
/// call of one becomes a call of an action, and what is said of a run that ended. It
/// holds no MCP type; the server holds the protocol.
pub trait Door: Send + Sync {
    /// The tools, in the order they are listed.
    fn tools(&self) -> Vec<Tool>;
    /// A call of the tool `name` with these arguments: the call of an action, and the
    /// caller's id for the request when it gave one; or why not, in words that say
    /// nothing the client sent.
    fn call(
        &self,
        name: &str,
        arguments: &Map<String, Json>,
    ) -> Result<(Call, Option<String>), String>;
    /// What the client is told of a run of the tool `name` that ended.
    fn said(&self, name: &str, run: &RunId, outcome: &Outcome) -> Said;
    /// The events a run that ended kept in its journal, when the door can read them:
    /// those it published after the server last looked reach the client too.
    fn journal(&self, _run: &RunId) -> Vec<JournalLine> {
        Vec::new()
    }
    /// The input ended, and every run the server started has ended or been given up:
    /// stop what the door holds.
    fn closing(&self) {}
}

/// The times and the bound the server keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// The stated time of a run: at it, the run is cancelled and its call is answered
    /// that the server gave up on it.
    pub wait: Duration,
    /// How often the runs are looked at between messages.
    pub look: Duration,
    /// How long a cancelled run is waited for: after the stated time, and at the end of
    /// the input.
    pub grace: Duration,
    /// The most calls that run at once.
    pub calls: usize,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits {
            wait: Duration::from_secs(900),
            look: Duration::from_millis(50),
            grace: Duration::from_secs(10),
            calls: CALLS_LIMIT,
        }
    }
}

/// A text as the server sends it: each control character escaped, everything else as
/// it is. Escaping twice is escaping once.
pub fn escaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_control() {
            out.extend(c.escape_debug());
        } else {
            out.push(c);
        }
    }
    out
}

/// One call the server started a run for.
struct Running {
    /// The id of the client's request, which the answer carries.
    id: Json,
    tool: String,
    run: RunId,
    token: Option<Json>,
    /// How many progress notifications were sent.
    next: u64,
    /// The place of the next of the run's events to look at.
    seen: u64,
    until: Instant,
    signal: Cancellation,
    /// When the server gave up on the run: at its stated time, or at the end of the
    /// input.
    gave_up: Option<Instant>,
    /// The client cancelled the call: nothing is answered (the protocol's rule).
    silent: bool,
}

struct Serving<'a, W: Write> {
    executor: &'a dyn Executor,
    door: &'a dyn Door,
    limits: Limits,
    out: W,
    running: Vec<Running>,
    initialized: bool,
}

/// Serves one client: reads `input` to its end, writes only the protocol to `output`,
/// and returns once every run it started has ended or been given up and the door was
/// told to stop. An error is one of writing the output, or of reading the input; the
/// runs are cancelled and the door told either way.
pub fn serve<R, W>(
    executor: Arc<dyn Executor>,
    door: Arc<dyn Door>,
    limits: Limits,
    input: R,
    output: W,
) -> io::Result<()>
where
    R: Read + Send + 'static,
    W: Write,
{
    let (send, messages) = sync_channel::<io::Result<Frame>>(QUEUED_LIMIT);
    let reader = crate::threads::spawn(Box::new(move || {
        let mut input = BufReader::new(input);
        loop {
            let frame = frame::next(&mut input);
            let last = !matches!(frame, Ok(ref read) if *read != Frame::End);
            if send.send(frame).is_err() || last {
                break;
            }
        }
    }))?;
    let mut serving = Serving {
        executor: executor.as_ref(),
        door: door.as_ref(),
        limits,
        out: output,
        running: Vec::new(),
        initialized: false,
    };
    let served = serving.serve(&messages);
    serving.close();
    drop(messages);
    // The reader has ended when the input did; when the server ended on a failure of
    // its output instead, the reader may still wait on its input, and is left to end
    // with it.
    if matches!(served, Ok(())) {
        let _ = reader.join();
    }
    served
}

impl<W: Write> Serving<'_, W> {
    fn serve(&mut self, messages: &Receiver<io::Result<Frame>>) -> io::Result<()> {
        loop {
            match messages.recv_timeout(self.limits.look) {
                Ok(Ok(Frame::End)) | Err(RecvTimeoutError::Disconnected) => return Ok(()),
                Ok(Err(error)) => return Err(error),
                Ok(Ok(frame)) => self.take(frame)?,
                Err(RecvTimeoutError::Timeout) => {}
            }
            self.look()?;
        }
    }

    fn write(&mut self, message: &Json) -> io::Result<()> {
        let mut line = serde_json::to_vec(message).map_err(io::Error::other)?;
        line.push(b'\n');
        self.out.write_all(&line)?;
        self.out.flush()
    }

    fn answer(&mut self, id: &Json, result: Json) -> io::Result<()> {
        self.write(&json!({"jsonrpc": "2.0", "id": id, "result": result}))
    }

    fn error(&mut self, id: &Json, code: i64, message: &str) -> io::Result<()> {
        let error = json!({"code": code, "message": message});
        self.write(&json!({"jsonrpc": "2.0", "id": id, "error": error}))
    }

    fn result(&mut self, id: &Json, text: &str, error: bool) -> io::Result<()> {
        // A result is lines: each escaped, the breaks between them kept.
        let lines: Vec<String> = text.split('\n').map(escaped).collect();
        let content = json!([{"type": "text", "text": lines.join("\n")}]);
        self.answer(id, json!({"content": content, "isError": error}))
    }

    fn take(&mut self, frame: Frame) -> io::Result<()> {
        let message = match frame {
            Frame::Message(message) => message,
            Frame::TooLarge => {
                let why = format!("a message is at most {} bytes", frame::MESSAGE_LIMIT);
                return self.error(&Json::Null, -32600, &why);
            }
            Frame::TooDeep => {
                let why = format!("a message nests at most {} deep", frame::DEPTH_LIMIT);
                return self.error(&Json::Null, -32600, &why);
            }
            Frame::NotJson | Frame::End => {
                return self.error(&Json::Null, -32700, "a message is a JSON object on a line");
            }
        };
        if message.get("jsonrpc") != Some(&json!("2.0")) {
            return self.error(&Json::Null, -32600, "a message says jsonrpc 2.0");
        }
        let Some(method) = message.get("method").and_then(Json::as_str) else {
            // A client's answer: the server asks nothing of a client, and drops it.
            return Ok(());
        };
        let params = match message.get("params") {
            None => Map::new(),
            Some(Json::Object(params)) => params.clone(),
            Some(_) => return self.error(&Json::Null, -32602, "params are an object"),
        };
        let Some(id) = message.get("id") else {
            self.notified(method, &params);
            return Ok(());
        };
        let id = match id {
            Json::String(text) if text.chars().count() <= ID_LIMIT => id.clone(),
            Json::Number(number) if number.is_i64() => id.clone(),
            _ => {
                let why = format!("an id is a whole number or a string of at most {ID_LIMIT}");
                return self.error(&Json::Null, -32600, &why);
            }
        };
        match method {
            "initialize" => {
                let asked = params.get("protocolVersion").and_then(Json::as_str);
                let version = PROTOCOL_VERSIONS
                    .into_iter()
                    .find(|known| Some(*known) == asked)
                    .unwrap_or(PROTOCOL_VERSIONS[0]);
                self.initialized = true;
                let result = json!({
                    "protocolVersion": version,
                    "capabilities": {"tools": {"listChanged": false}},
                    "serverInfo": {"name": "whizbang", "version": env!("CARGO_PKG_VERSION")},
                });
                self.answer(&id, result)
            }
            "ping" => self.answer(&id, json!({})),
            "tools/list" | "tools/call" if !self.initialized => {
                self.error(&id, -32002, "the server was not initialized")
            }
            "tools/list" => {
                let tools: Vec<Json> = (self.door.tools().into_iter())
                    .map(|tool| {
                        json!({"name": tool.name, "description": tool.description,
                               "inputSchema": tool.schema})
                    })
                    .collect();
                self.answer(&id, json!({"tools": tools}))
            }
            "tools/call" => self.call(id, &params),
            _ => self.error(&id, -32601, "the server has no such method"),
        }
    }

    fn notified(&mut self, method: &str, params: &Map<String, Json>) {
        if method != "notifications/cancelled" {
            return;
        }
        let Some(id) = params.get("requestId") else {
            return;
        };
        for running in self.running.iter_mut().filter(|r| &r.id == id) {
            running.silent = true;
            running.signal.cancel();
            // How the cancel was answered is seen when the run is looked at next.
            let _ = self.executor.cancel(&running.run);
        }
    }

    fn call(&mut self, id: Json, params: &Map<String, Json>) -> io::Result<()> {
        if self.running.iter().any(|running| running.id == id) {
            return self.error(&id, -32600, "a call under this id is running");
        }
        let Some(name) = params.get("name").and_then(Json::as_str) else {
            return self.error(&id, -32602, "a call names its tool");
        };
        let arguments = match params.get("arguments") {
            None => Map::new(),
            Some(Json::Object(arguments)) => arguments.clone(),
            Some(_) => return self.error(&id, -32602, "a call's arguments are an object"),
        };
        if !self.door.tools().iter().any(|tool| tool.name == name) {
            return self.error(&id, -32602, "the server offers no tool of that name");
        }
        let name = name.to_owned();
        let token = params
            .get("_meta")
            .and_then(|meta| meta.get("progressToken"))
            .filter(|token| match token {
                Json::String(text) => text.chars().count() <= ID_LIMIT,
                Json::Number(number) => number.is_i64(),
                _ => false,
            })
            .cloned();
        if self.running.len() >= self.limits.calls {
            let why = format!(
                "the server is busy: at most {} calls run at once",
                self.limits.calls
            );
            return self.result(&id, &why, true);
        }
        let (call, request_id) = match self.door.call(&name, &arguments) {
            Ok(asked) => asked,
            Err(why) => return self.result(&id, &why, true),
        };
        let signal = Cancellation::new();
        let until = Instant::now() + self.limits.wait;
        let mut request = Request::new(call).cancelled_by(signal.clone()).until(until);
        if let Some(text) = request_id {
            match self.executor.request_id(&text) {
                Ok(request_id) => request = request.with_id(request_id),
                Err(refused) => return self.result(&id, &refused.to_string(), true),
            }
        }
        let run = match self.executor.start(&request) {
            Ok(Answer::Started(run)) => run,
            Ok(Answer::Ran(finished)) => {
                let said = self.door.said(&name, finished.run(), finished.outcome());
                return self.result(&id, &said.text, said.error);
            }
            Ok(Answer::Repeat(earlier)) => {
                let shown = escaped(earlier.run().as_str());
                return match earlier.state() {
                    RunState::Ended(outcome) => {
                        let said = self.door.said(&name, earlier.run(), outcome);
                        let text = format!(
                            "the request was sent before, and nothing ran again\n{}",
                            said.text
                        );
                        self.result(&id, &text, said.error)
                    }
                    _ => {
                        let text = format!(
                            "the request was sent before as run {shown}, which has not ended here"
                        );
                        self.result(&id, &text, true)
                    }
                };
            }
            Err(refused) => return self.result(&id, &refused.to_string(), true),
        };
        let mut running = Running {
            id,
            tool: name,
            run,
            token,
            next: 0,
            seen: 0,
            until,
            signal,
            gave_up: None,
            silent: false,
        };
        let started = format!("run {} started", running.run.as_str());
        self.progress(&mut running, &started)?;
        self.running.push(running);
        Ok(())
    }

    fn progress(&mut self, running: &mut Running, message: &str) -> io::Result<()> {
        let Some(token) = &running.token else {
            return Ok(());
        };
        running.next += 1;
        let params = json!({"progressToken": token, "progress": running.next,
                            "message": escaped(message)});
        let note = json!({"jsonrpc": "2.0", "method": "notifications/progress", "params": params});
        self.write(&note)
    }

    /// Sends the events of a run from the place it got to: those `status` gave while
    /// it ran, or its journal once it ended.
    fn events(&mut self, running: &mut Running, events: &[JournalLine]) -> io::Result<()> {
        for event in events {
            if event.seq() < running.seen {
                continue;
            }
            running.seen = event.seq() + 1;
            // A typed event is for whoever shows a run's steps (plan m5, task K1): a
            // client is sent the progress messages it was always sent, and no more.
            if event.event().is_none() {
                self.progress(running, event.message())?;
            }
        }
        Ok(())
    }

    /// Looks at each run once: its new events, its end, and its stated time.
    fn look(&mut self) -> io::Result<()> {
        let now = Instant::now();
        let mut left = Vec::new();
        for mut running in std::mem::take(&mut self.running) {
            // Only the events after those already looked at are copied.
            let state = match self.executor.status_after(&running.run, running.seen) {
                Ok(watched) => {
                    self.events(&mut running, watched.events())?;
                    watched.state().clone()
                }
                Err(refused) => {
                    if !running.silent {
                        self.result(&running.id.clone(), &refused.to_string(), true)?;
                    }
                    continue;
                }
            };
            match state {
                RunState::Ended(outcome) => {
                    // A run whose own deadline, at the same stated time, ended it before
                    // the server looked was given up all the same: it is answered so,
                    // whichever of the two came first (the review of task M1: a race
                    // seen once on the macOS runner).
                    if running.gave_up.is_none()
                        && now >= running.until
                        && outcome.name() != "completed"
                    {
                        running.gave_up = Some(now);
                    }
                    let journal = self.door.journal(&running.run);
                    self.events(&mut running, &journal)?;
                    if !running.silent {
                        self.ended(&running, &outcome)?;
                    }
                }
                RunState::NotEnded { here: true } => match running.gave_up {
                    None if now >= running.until => {
                        running.signal.cancel();
                        let _ = self.executor.cancel(&running.run);
                        running.gave_up = Some(now);
                        left.push(running);
                    }
                    Some(at) if now.duration_since(at) >= self.limits.grace => {
                        if !running.silent {
                            let text = format!(
                                "gave up on run {} after its stated time of {} s; it was cancelled and has not ended",
                                running.run.as_str(),
                                self.limits.wait.as_secs_f64()
                            );
                            self.result(&running.id.clone(), &text, true)?;
                        }
                    }
                    _ => left.push(running),
                },
                other => {
                    if !running.silent {
                        let why = match other {
                            RunState::Removed { .. } => "is no longer in the store",
                            RunState::Abandoned { .. } => "was admitted and will not end here",
                            _ => "is not running in this process",
                        };
                        let text = format!("run {} {why}", running.run.as_str());
                        self.result(&running.id.clone(), &text, true)?;
                    }
                }
            }
        }
        left.append(&mut self.running);
        self.running = left;
        Ok(())
    }

    fn ended(&mut self, running: &Running, outcome: &Outcome) -> io::Result<()> {
        let said = self.door.said(&running.tool, &running.run, outcome);
        let text = match running.gave_up {
            Some(_) => format!(
                "gave up on run {} after its stated time of {} s, and cancelled it\n{}",
                running.run.as_str(),
                self.limits.wait.as_secs_f64(),
                said.text
            ),
            None => said.text,
        };
        let error = said.error || running.gave_up.is_some();
        self.result(&running.id.clone(), &text, error)
    }

    /// The end of the input: every run still going is cancelled and waited for, up to
    /// the grace, and the door is told to stop. Nothing more is written: the client
    /// has gone.
    fn close(&mut self) {
        for running in &self.running {
            running.signal.cancel();
            let _ = self.executor.cancel(&running.run);
        }
        let until = Instant::now() + self.limits.grace;
        while !self.running.is_empty() && Instant::now() < until {
            self.running.retain(|running| {
                let state = self.executor.status(&running.run);
                matches!(state, Ok(watched) if matches!(watched.state(), RunState::NotEnded { here: true }))
            });
            if !self.running.is_empty() {
                std::thread::sleep(self.limits.look);
            }
        }
        self.running.clear();
        self.door.closing();
    }
}
