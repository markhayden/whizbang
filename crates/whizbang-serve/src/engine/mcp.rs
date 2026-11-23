//! `POST /mcp` (ADR 0045, "MCP's streamable HTTP transport"): the short answers, as one
//! JSON body; a `tools/call` that made a run, answered as an event stream that names the
//! run first, then its progress, then its result; and a repeat of a run answered as its
//! result. The tools, and how a call of one becomes a call of an action, are the stdio
//! server's door's.

use std::net::{Shutdown, TcpStream};
use std::sync::Arc;
use std::time::Instant;

use serde_json::{Value as Json, json};
use whizbang_core::contract::{Answer, CallError, Refusal, Request, RunId, RunState};

use super::guard::{Asked, Guarded, deadlines};
use super::{Follower, Held, PROTOCOL_VERSION, RUN_HEADER, RUN_META, Shared};
use crate::escaped;
use crate::http::{self, Response};

/// A JSON-RPC answer.
pub fn rpc_answer(id: &Json, result: Json) -> Json {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

/// A JSON-RPC error.
pub fn rpc_error(id: &Json, code: i64, message: &str) -> Json {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

/// A tool's result: its text, each line escaped, whether it is an error, and the run it
/// is of in its `_meta`.
pub fn tool_result(text: &str, error: bool, run: Option<&RunId>) -> Json {
    let lines: Vec<String> = text.split('\n').map(escaped).collect();
    let mut result = json!({"content": [{"type": "text", "text": lines.join("\n")}],
                            "isError": error});
    if let Some(run) = run {
        result["_meta"] = json!({ RUN_META: run.as_str() });
    }
    result
}

/// A tool's result as a JSON-RPC answer.
pub fn tool_answer(id: &Json, text: &str, error: bool, run: Option<&RunId>) -> Json {
    rpc_answer(id, tool_result(text, error, run))
}

/// Writes one JSON answer, naming its run in the header when it is of one.
fn send_json(shared: &Shared, conn: &mut TcpStream, answer: &Json, run: Option<&RunId>) {
    let mut response = Response::json(200, answer);
    if let Some(run) = run {
        response = response.with(RUN_HEADER, run.as_str());
    }
    response.send(conn, &deadlines(shared));
}

/// The short answers of `POST /mcp`, on a short handler.
pub(crate) fn short(shared: &Arc<Shared>, guarded: Guarded) {
    let Guarded {
        mut conn, asked, ..
    } = guarded;
    let answer = match asked {
        Asked::Initialize { id } => rpc_answer(
            &id,
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "whizbang", "version": env!("CARGO_PKG_VERSION")},
            }),
        ),
        Asked::Ping { id } => rpc_answer(&id, json!({})),
        Asked::List { id } => {
            let tools: Vec<Json> = (shared.hosted.tools().into_iter())
                .map(|tool| {
                    json!({"name": tool.name, "description": tool.description,
                           "inputSchema": tool.schema})
                })
                .collect();
            rpc_answer(&id, json!({ "tools": tools }))
        }
        Asked::Unknown { id } => rpc_error(&id, -32601, "the server has no such method"),
        Asked::Repeat { id, tool, run, .. } => return repeated(shared, conn, &id, &tool, &run),
        _ => return,
    };
    send_json(shared, &mut conn, &answer, None);
}

/// A call whose request id names a run already made that no handler here follows,
/// answered as one body: its result when it has ended. Nothing runs again.
fn repeated(shared: &Shared, mut conn: TcpStream, id: &Json, tool: &str, run: &RunId) {
    let Some(executor) = shared.executor() else {
        return Response::refused(503, "the engine is stopping")
            .send(&mut conn, &deadlines(shared));
    };
    let status = executor.status_after(run, u64::MAX);
    drop(executor);
    let shown = escaped(run.as_str());
    let (text, error) = match status {
        Ok(watched) => match watched.state() {
            RunState::Ended(outcome) => {
                let said = shared.hosted.said(tool, run, outcome);
                (said.text, said.error)
            }
            RunState::NotEnded { here: true } => (
                format!(
                    "run {shown} is still going here; it was given up on at its stated time, and was cancelled"
                ),
                true,
            ),
            RunState::Removed { .. } => (
                format!(
                    "run {shown} was made by this request before and is no longer in the store; nothing ran again"
                ),
                true,
            ),
            _ => (
                format!(
                    "run {shown} was made by this request before and will not end here; nothing ran again"
                ),
                true,
            ),
        },
        Err(CallError::Refused(Refusal::NoSuchRun { .. })) => (
            format!(
                "run {shown} was made by this request before and is no longer in the store; nothing ran again"
            ),
            true,
        ),
        Err(error) => (error.to_string(), true),
    };
    send_json(
        shared,
        &mut conn,
        &tool_answer(id, &text, error, Some(run)),
        Some(run),
    );
}

/// The stream a run's progress and result are written to: the caller that started it, or
/// the latest that followed it again.
struct Out {
    conn: TcpStream,
    id: Json,
    token: Option<Json>,
    /// How many progress notifications were sent.
    next: u64,
    /// Whether the caller still takes what is written.
    open: bool,
    _held: Option<Held>,
}

impl Out {
    fn new(conn: TcpStream, id: Json, token: Option<Json>, held: Option<Held>) -> Out {
        Out {
            conn,
            id,
            token,
            next: 0,
            open: true,
            _held: held,
        }
    }

    fn of(follower: Follower) -> Out {
        Out::new(follower.conn, follower.id, follower.token, follower.held)
    }

    /// The head: the stream's type, and its run.
    fn begin(&mut self, shared: &Shared, run: &RunId) {
        let head = http::stream_head(
            200,
            "text/event-stream",
            &[(RUN_HEADER, run.as_str().to_owned())],
        );
        self.open = http::write_within(&mut self.conn, &head, shared.settings.times.piece);
    }

    fn event(&mut self, shared: &Shared, message: &Json) {
        if !self.open {
            return;
        }
        let piece = format!("event: message\ndata: {message}\n\n");
        self.open = http::write_within(
            &mut self.conn,
            piece.as_bytes(),
            shared.settings.times.piece,
        );
    }

    /// A progress message, under the caller's token when it gave one; none when it gave
    /// none, as over stdio.
    fn progress(&mut self, shared: &Shared, message: &str) {
        let Some(token) = self.token.clone() else {
            return;
        };
        self.next += 1;
        let params = json!({"progressToken": token, "progress": self.next,
                            "message": escaped(message)});
        let note = json!({"jsonrpc": "2.0", "method": "notifications/progress", "params": params});
        self.event(shared, &note);
    }

    /// The result, and the end of the stream.
    fn end(mut self, shared: &Shared, result: Json) {
        let answer = rpc_answer(&self.id, result);
        self.event(shared, &answer);
        if self.open {
            http::close(&mut self.conn);
        }
    }

    /// Closed, unanswered: another caller took the run over.
    fn drop_conn(self) {
        let _ = self.conn.shutdown(Shutdown::Both);
    }
}

/// A `tools/call` that took a place: the run started, then followed to its end on this
/// handler.
pub(crate) fn stream(shared: &Arc<Shared>, guarded: Guarded) {
    let Guarded {
        mut conn,
        asked,
        held,
        ..
    } = guarded;
    let Asked::Call {
        id,
        tool,
        token,
        call,
        request,
        place,
    } = asked
    else {
        return;
    };
    let Some(executor) = shared.executor() else {
        return Response::refused(503, "the engine is stopping")
            .send(&mut conn, &deadlines(shared));
    };
    let until = place.until;
    // At its stated time the run is cancelled, and its children with it. Its own deadline
    // comes a grace later: a backstop for an action that reads its deadline and not its
    // cancellation, which does not race the cancel.
    let mut asked = Request::new(call)
        .cancelled_by(place.signal.clone())
        .until(until + shared.settings.grace);
    if let Some(request) = request {
        asked = asked.with_id(request);
    }
    let answer = executor.start(&asked);
    drop(executor);
    match answer {
        Ok(Answer::Started(run)) => {
            place.made(&run);
            let out = Out::new(conn, id, token, Some(held));
            follow(shared, &run, &tool, out, until);
        }
        Ok(Answer::Ran(finished)) => {
            drop(place);
            let said = shared
                .hosted
                .said(&tool, finished.run(), finished.outcome());
            let answer = tool_answer(&id, &said.text, said.error, Some(finished.run()));
            send_json(shared, &mut conn, &answer, Some(finished.run()));
        }
        Ok(Answer::Repeat(earlier)) => {
            // The same request, sent twice at once: the second follows the first's run.
            drop(place);
            let run = earlier.run().clone();
            let follower = Follower {
                conn,
                id,
                token,
                held: Some(held),
            };
            if let Err(back) = shared.attach(&run, follower) {
                repeated(shared, back.conn, &back.id, &tool, &run);
            }
        }
        Err(refused) => {
            drop(place);
            let answer = tool_answer(&id, &refused.to_string(), true, None);
            send_json(shared, &mut conn, &answer, None);
        }
    }
}

/// Follows a run this handler started, to its end: its progress to whoever follows it
/// now, a repeat's connection taken over, its stated time held, and its result.
fn follow(shared: &Arc<Shared>, run: &RunId, tool: &str, out: Out, until: Instant) {
    let mut out = out;
    out.begin(shared, run);
    out.progress(shared, &format!("run {} started", run.as_str()));
    let mut seen = 0u64;
    let mut gave_up: Option<Instant> = None;
    let wait_s = shared.settings.wait.as_secs_f64();
    loop {
        if shared.done() {
            return;
        }
        if let Some(follower) = shared.take_follower(run) {
            out.drop_conn();
            out = Out::of(follower);
            out.begin(shared, run);
        }
        let Some(executor) = shared.executor() else {
            return;
        };
        let watched = executor.status_after(run, seen);
        drop(executor);
        let watched = match watched {
            Ok(watched) => watched,
            Err(refused) => {
                shared.ended(run);
                return out.end(shared, tool_result(&refused.to_string(), true, Some(run)));
            }
        };
        for event in watched.events() {
            if event.seq() < seen {
                continue;
            }
            seen = event.seq() + 1;
            // A typed event is for whoever shows a run's steps: a caller is sent the
            // progress messages, as over stdio.
            if event.event().is_none() {
                out.progress(shared, event.message());
            }
        }
        let now = Instant::now();
        match watched.state().clone() {
            RunState::Ended(outcome) => {
                for event in shared.hosted.journal(run) {
                    if event.seq() >= seen && event.event().is_none() {
                        seen = event.seq() + 1;
                        out.progress(shared, event.message());
                    }
                }
                // A run whose own deadline ended it first was given up all the same.
                if gave_up.is_none() && now >= until && outcome.name() != "completed" {
                    gave_up = Some(now);
                }
                if let Some(follower) = shared.ended(run) {
                    out.drop_conn();
                    out = Out::of(follower);
                    out.begin(shared, run);
                }
                let said = shared.hosted.said(tool, run, &outcome);
                let text = match gave_up {
                    Some(_) => format!(
                        "gave up on run {} after its stated time of {wait_s} s, and cancelled it\n{}",
                        run.as_str(),
                        said.text
                    ),
                    None => said.text,
                };
                let error = said.error || gave_up.is_some();
                return out.end(shared, tool_result(&text, error, Some(run)));
            }
            RunState::NotEnded { here: true } => match gave_up {
                None if now >= until => {
                    shared.stop(run);
                    gave_up = Some(now);
                }
                Some(at) if now.duration_since(at) >= shared.settings.grace => {
                    shared.unfollow(run);
                    let text = format!(
                        "gave up on run {} after its stated time of {wait_s} s; it was cancelled and has not ended",
                        run.as_str()
                    );
                    return out.end(shared, tool_result(&text, true, Some(run)));
                }
                _ => {}
            },
            other => {
                shared.ended(run);
                let why = match other {
                    RunState::Removed { .. } => "is no longer in the store",
                    RunState::Abandoned { .. } => "was admitted and will not end here",
                    _ => "is not running in this process",
                };
                let text = format!("run {} {why}", run.as_str());
                return out.end(shared, tool_result(&text, true, Some(run)));
            }
        }
        let state = shared.state();
        let waiting = (state.places.iter())
            .any(|place| place.run.as_ref() == Some(run) && place.follower.is_some());
        if !waiting && !state.done {
            drop(shared.wait_a_look(state));
        }
    }
}
