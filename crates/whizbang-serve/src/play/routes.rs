//! The routes (ADR 0041, "Routes"): a fixed table, each row a method, a pattern of the
//! path, what body and query fields it takes, and a handler that is given only a
//! [`Guarded`] request, which only [`super::guard::admit`] makes. Each later task adds its
//! rows here, one line each.

use std::net::TcpStream;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use serde_json::{Map, Value as Json, json};
use whizbang_core::contract::{
    Answer, CallError, Cancellation, PreparedCall, Readiness, Refusal, Request, RunId, RunState,
    Watched,
};

use super::guard::{self, Guarded};
use super::http::{PAGE_POLICY, Response, Unread};
use super::{Asked, Kept, PREPARATIONS_LIMIT, Shared, arguments_digest, assets};
use crate::escaped;

/// What body a route takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Body {
    None,
    Json,
    Picture,
}

/// A handler: given the host and a request that passed the guard.
pub(crate) type Handler = fn(&Shared, &Guarded<'_>) -> Response;

/// One row of the table.
pub(crate) struct Route {
    pub method: &'static str,
    /// Segments split at `/`; `{id}` names one, `*` any path outside `/api/`.
    pub pattern: &'static str,
    pub body: Body,
    /// The query's fields it takes; any other is refused.
    pub query: &'static [&'static str],
    pub handler: Handler,
}

/// The header a Chrome case sets the fixture host's script by; the real host refuses it.
pub const SCRIPT_HEADER: &str = "x-fixture-script";

/// The table. The static files come last: they answer any path outside `/api/`.
pub(crate) const ROUTES: &[Route] = &[
    Route {
        method: "GET",
        pattern: "/api/hello",
        body: Body::None,
        query: &[],
        handler: hello,
    },
    Route {
        method: "POST",
        pattern: "/api/quit",
        body: Body::None,
        query: &[],
        handler: quit,
    },
    Route {
        method: "POST",
        pattern: "/api/prepare",
        body: Body::Json,
        query: &[],
        handler: prepare,
    },
    Route {
        method: "POST",
        pattern: "/api/runs",
        body: Body::Json,
        query: &[],
        handler: start,
    },
    Route {
        method: "GET",
        pattern: "/api/runs",
        body: Body::None,
        query: super::history::LISTED,
        handler: super::history::runs,
    },
    Route {
        method: "GET",
        pattern: "/api/runs/{id}",
        body: Body::None,
        query: &["after"],
        handler: status,
    },
    Route {
        method: "POST",
        pattern: "/api/runs/{id}/cancel",
        body: Body::None,
        query: &[],
        handler: stop,
    },
    Route {
        method: "POST",
        pattern: "/api/pictures",
        body: Body::Picture,
        query: &["name"],
        handler: picture,
    },
    Route {
        method: "GET",
        pattern: "/api/pictures",
        body: Body::None,
        query: &["after", "limit"],
        handler: pictures,
    },
    Route {
        method: "GET",
        pattern: "/api/artifacts/{id}",
        body: Body::None,
        query: &[],
        handler: artifact,
    },
    Route {
        method: "GET",
        pattern: "/api/artifacts/{id}/thumb",
        body: Body::None,
        query: &["edge"],
        handler: thumb,
    },
    Route {
        method: "POST",
        pattern: "/api/locations",
        body: Body::Json,
        query: &[],
        handler: location,
    },
    Route {
        method: "GET",
        pattern: "*",
        body: Body::None,
        query: &[],
        handler: asset,
    },
];

/// The tables: this one, then each later task's, one line each. The static files'
/// route matches no path under `/api/`, so no route of a later table is hidden by it.
const TABLES: &[&[Route]] = &[ROUTES, super::history::ROUTES];

/// The route of this method and path, with the parts of the path its pattern names: a
/// path no route has is 404, and one whose route takes another method 405.
pub(crate) fn find(method: &str, path: &str) -> Result<(&'static Route, Vec<String>), Unread> {
    let mut path_known = false;
    for route in TABLES.iter().flat_map(|table| table.iter()) {
        let Some(params) = matched(route.pattern, path) else {
            continue;
        };
        if route.method == method {
            return Ok((route, params));
        }
        path_known = true;
    }
    let (status, why) = match path_known {
        true => (405, "this route takes another method"),
        false => (404, "no such route"),
    };
    Err(Unread::Answer(Response::refused(status, why)))
}

fn matched(pattern: &str, path: &str) -> Option<Vec<String>> {
    if pattern == "*" {
        let api = path == "/api" || path.starts_with("/api/");
        return (!api).then(Vec::new);
    }
    let mut params = Vec::new();
    let mut wanted = pattern.split('/');
    let mut given = path.split('/');
    loop {
        match (wanted.next(), given.next()) {
            (None, None) => return Some(params),
            (Some("{id}"), Some(part)) if !part.is_empty() => params.push(part.to_owned()),
            (Some(want), Some(part)) if want == part => {}
            _ => return None,
        }
    }
}

/// Serves one connection: the guard, then the route's handler, then the answer.
pub(crate) fn serve(shared: &Shared, stream: &mut TcpStream, since: Instant) {
    let deadlines = shared.settings.deadlines;
    match guard::admit(shared, stream, since) {
        Ok((guarded, route)) => {
            // A script of the fixture host's: the real host refuses the header, which
            // no page of its own sends (plan m5, task U1, item 5).
            let scripted = match guarded.head.header(SCRIPT_HEADER) {
                Some(words) => shared.play.script(words).err(),
                None => None,
            };
            let response = match scripted {
                Some(why) => Response::refused(400, &escaped(&why)),
                None => (route.handler)(shared, &guarded),
            };
            drop(guarded);
            response.send(stream, &deadlines);
        }
        Err(Unread::Answer(response)) => response.send(stream, &deadlines),
        Err(Unread::Close) => {}
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
        })
}

fn hello(shared: &Shared, _: &Guarded<'_>) -> Response {
    let mut said = Map::new();
    said.insert("name".to_owned(), json!("whizbang"));
    said.insert("version".to_owned(), json!(env!("CARGO_PKG_VERSION")));
    said.insert(
        "store".to_owned(),
        json!(escaped(&shared.settings.store.to_string_lossy())),
    );
    for (name, value) in shared.play.hello() {
        said.entry(name).or_insert(value);
    }
    Response::json(200, &Json::Object(said))
}

fn quit(shared: &Shared, _: &Guarded<'_>) -> Response {
    shared.quit();
    Response::json(202, &json!({"stopping": true}))
}

/// The body of a preparation: an action and its arguments, and nothing else.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Preparing {
    action: String,
    #[serde(default)]
    arguments: Map<String, Json>,
}

/// The body of a start: the call, the handle of its preparation, and the page's id.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Starting {
    action: String,
    #[serde(default)]
    arguments: Map<String, Json>,
    #[serde(default)]
    handle: Option<String>,
    #[serde(default)]
    request_id: Option<String>,
}

fn body<T: for<'d> Deserialize<'d>>(guarded: &Guarded<'_>) -> Result<T, Response> {
    let json = guarded.json.clone().unwrap_or(Json::Null);
    serde_json::from_value(json).map_err(|_| {
        Response::refused(
            400,
            "the body names a field this route does not take, or lacks one",
        )
    })
}

/// A refusal of the executor's, as the page is told it: the sentence, escaped, and its
/// kind, for a refusal of the call; fixed words for a failure that is not the call's.
fn call_refused(error: &CallError) -> Response {
    match error {
        CallError::Refused(refusal) => {
            let status = match refusal {
                Refusal::Stale { .. } | Refusal::Conflict { .. } => 409,
                Refusal::NoSuchRun { .. } => 404,
                _ => 422,
            };
            Response::json(
                status,
                &json!({"error": escaped(&refusal.to_string()), "kind": kind_word(refusal)}),
            )
        }
        _ => Response::refused(500, "the store could not answer"),
    }
}

fn kind_word(refusal: &Refusal) -> String {
    let debug = format!("{:?}", refusal.kind());
    let mut word = String::new();
    for (at, c) in debug.chars().enumerate() {
        if c.is_ascii_uppercase() && at > 0 {
            word.push('_');
        }
        word.push(c.to_ascii_lowercase());
    }
    word
}

/// Millionths of a US dollar as dollars, for the page.
#[allow(clippy::cast_precision_loss)]
fn dollars(micros: u64) -> f64 {
    micros as f64 / 1_000_000.0
}

/// What a preparation says of each need: its capability, the backend the call chose (or
/// the model's), the model, its request count and its bounds (plan m5, task K1, item 2).
fn needs_json(shared: &Shared, prepared: &PreparedCall) -> Vec<Json> {
    prepared
        .needs()
        .iter()
        .map(|need| {
            let each = need.each().map_or(0, |each| each.micros());
            let bound = need.bound().map_or(0, |bound| bound.micros());
            // The config's name of the backend that answers: the call's choice, else
            // the capability's default, as preparation resolved it (ADR 0006).
            let backend = need
                .choice()
                .map(str::to_owned)
                .or_else(|| shared.play.default_backend(need.capability().name()))
                .or_else(|| need.model().map(|model| model.backend().to_owned()));
            json!({
                "capability": need.capability().name(),
                "backend": backend.map(|backend| escaped(&backend)),
                "model": need.model().map(|model| escaped(model.model())),
                "requests": need.requests(),
                "each": dollars(each),
                "bound": dollars(bound),
                "local": each == 0,
            })
        })
        .collect()
}

/// Why a preparation is not ready: each capability nothing answers, with its kind.
fn missing_json(prepared: &PreparedCall) -> Vec<Json> {
    match prepared.readiness() {
        Readiness::Ready => Vec::new(),
        Readiness::NotReady(missing) => missing
            .iter()
            .map(|missing| {
                json!({
                    "capability": missing.capability().name(),
                    "kind": missing.kind().word(),
                    "why": escaped(missing.why()),
                })
            })
            .collect(),
    }
}

/// One choice's readiness, from its own preparation: `ready`, or the kind of the first
/// capability nothing answers, or the kind of the refusal.
fn choice_json(value: &str, prepared: Result<PreparedCall, CallError>) -> Json {
    match prepared {
        Ok(prepared) => match prepared.readiness() {
            Readiness::Ready => {
                json!({"value": value, "ready": true, "kind": "ready", "why": null})
            }
            Readiness::NotReady(missing) => {
                let first = missing.first();
                json!({"value": value, "ready": false,
                       "kind": first.map_or("other", |m| m.kind().word()),
                       "why": first.map(|m| escaped(m.why()))})
            }
        },
        Err(CallError::Refused(refusal)) => {
            json!({"value": value, "ready": false, "kind": kind_word(&refusal),
                   "why": escaped(&refusal.to_string())})
        }
        Err(_) => json!({"value": value, "ready": false, "kind": "other", "why": null}),
    }
}

fn prepare(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    let asked: Preparing = match body(guarded) {
        Ok(asked) => asked,
        Err(refused) => return refused,
    };
    let (call, _) = match shared.play.call(&asked.action, &asked.arguments) {
        Ok(made) => made,
        Err(why) => return Response::refused(422, &escaped(&why)),
    };
    let Some(executor) = shared.executor() else {
        return Response::refused(503, "the playground is stopping");
    };
    // Each choice of a backend, prepared once each with the rest of the call as given:
    // its readiness is the preparation's, by kind (plan m5, task W1, item 4).
    let mut choices = Map::new();
    for (argument, values) in shared.play.choosers(&asked.action) {
        let mut answers = Vec::new();
        for backend in values {
            let Some(value) = backend["name"].as_str() else {
                continue;
            };
            let mut arguments = asked.arguments.clone();
            arguments.insert(argument.clone(), json!(value));
            let prepared = match shared.play.call(&asked.action, &arguments) {
                Ok((call, _)) => executor.prepare(&call),
                Err(_) => continue,
            };
            let mut answer = choice_json(value, prepared);
            // A backend that is not ready for a reason of its own (a model not
            // installed, a key not set) says that reason, which the preparation may
            // only see as a feature it cannot declare.
            if answer["kind"] == "other"
                && let Some(own) = backend["missing"].as_str()
            {
                answer["kind"] = json!(own);
            }
            answer["size"] = backend.get("size").cloned().unwrap_or(Json::Null);
            answers.push(answer);
        }
        choices.insert(argument, Json::Array(answers));
    }
    match executor.prepare(&call) {
        Ok(prepared) => {
            let ready = matches!(prepared.readiness(), Readiness::Ready);
            let mut said = json!({
                "ready": ready,
                "refused": false,
                "kind": null,
                "why": null,
                "handle": null,
                "missing": missing_json(&prepared),
                "needs": needs_json(shared, &prepared),
                "bound": dollars(prepared.estimate().micros()),
                "attempts": prepared.plan().and_then(|plan| plan.stated()).map(|attempts| {
                    json!({"asked": attempts.asked, "allowed": attempts.allowed})
                }),
                "choices": Json::Object(choices),
            });
            if !ready {
                return Response::json(200, &said);
            }
            let Ok(handle) = super::token::random_hex(16) else {
                return Response::refused(500, "the system's randomness could not be read");
            };
            let mut state = shared.state();
            let now = Instant::now();
            let age = shared.settings.preparation_age;
            state
                .preparations
                .retain(|kept| now.duration_since(kept.at) < age);
            while state.preparations.len() >= PREPARATIONS_LIMIT {
                state.preparations.pop_front();
            }
            state.preparations.push_back(Kept {
                handle: handle.clone(),
                prepared: Arc::new(prepared),
                at: now,
            });
            said["handle"] = json!(handle);
            Response::json(200, &said)
        }
        Err(CallError::Refused(refusal)) => Response::json(
            200,
            &json!({"ready": false, "refused": true, "kind": kind_word(&refusal),
                    "why": escaped(&refusal.to_string()), "handle": null,
                    "missing": [], "needs": [], "bound": 0, "attempts": null,
                    "choices": Json::Object(choices)}),
        ),
        Err(_) => Response::refused(500, "the store could not answer"),
    }
}

fn start(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    let asked: Starting = match body(guarded) {
        Ok(asked) => asked,
        Err(refused) => return refused,
    };
    let Some(text_id) = asked.request_id.clone() else {
        return Response::refused(400, "a start names its request_id, of the page's making");
    };
    let Some(executor) = shared.executor() else {
        return Response::refused(503, "the playground is stopping");
    };
    // A repeated id is answered before anything else is looked at (ADR 0007): the run
    // it started, or a conflict when something else was asked under it.
    let earlier = shared.state().requests.get(&text_id).map(|earlier| {
        let same = earlier.action == asked.action
            && earlier.arguments == arguments_digest(&asked.arguments);
        (earlier.run.clone(), same)
    });
    if let Some((run, same)) = earlier {
        if !same {
            return Response::json(
                409,
                &json!({"error": "another call was started under this request id",
                        "kind": "conflict"}),
            );
        }
        let state = executor
            .status_after(&run, u64::MAX)
            .map_or("running", |watched| state_word(watched.state(), false));
        return Response::json(
            200,
            &json!({"run": run.as_str(), "repeat": true, "state": state}),
        );
    }
    let id = match executor.request_id(&text_id) {
        Ok(id) => id,
        Err(error) => return call_refused(&error),
    };
    // An id the host has forgotten (it restarted) and the store holds: answered with the
    // run its admission names, by that id alone, before a place is taken or the call
    // is made again (ADR 0007). The call is not compared: after a restart the host
    // holds no copy of what was asked, and a call made again under a changed config
    // would not be the one the id was given for. A request id is the page's, one for
    // each Run pressed.
    if let Some(run) = shared.play.admitted(&id) {
        let state = executor
            .status_after(&run, u64::MAX)
            .map_or("removed", |watched| state_word(watched.state(), false));
        return Response::json(
            200,
            &json!({"run": run.as_str(), "repeat": true, "state": state}),
        );
    }
    // A place among the calls, held while the start is admitted, and given back if it
    // makes no run.
    let signal = Cancellation::new();
    let until = Instant::now() + shared.settings.run_limit;
    let Some(admitting) = shared.admit(&asked.action, &asked.arguments, &signal, until, now_ms())
    else {
        if shared.stopping.load(std::sync::atomic::Ordering::SeqCst) {
            return Response::refused(503, "the playground is stopping");
        }
        let why = format!(
            "at most {} runs started here go at once; try again when one ends",
            shared.settings.calls
        );
        return Response::refused(429, &why).with("Retry-After", "5");
    };
    let (call, _) = match shared.play.call(&asked.action, &asked.arguments) {
        Ok(made) => made,
        Err(why) => return Response::refused(422, &escaped(&why)),
    };
    let age = shared.settings.preparation_age;
    let prepared = asked.handle.as_ref().and_then(|handle| {
        let now = Instant::now();
        let state = shared.state();
        state
            .preparations
            .iter()
            .find(|kept| &kept.handle == handle && now.duration_since(kept.at) < age)
            .map(|kept| kept.prepared.clone())
    });
    // At its stated time the keeper cancels the run, and its children with it. The
    // run's own deadline comes a grace later: a backstop for an action that reads its
    // deadline and not its cancellation, which does not race the cancel.
    let mut request = Request::new(call)
        .cancelled_by(signal)
        .until(until + shared.settings.grace)
        .needing_preparation()
        .with_id(id);
    if let Some(prepared) = &prepared {
        request = request.expecting(prepared);
    }
    match executor.start(&request) {
        Ok(Answer::Started(run)) => {
            admitting.made(run.clone());
            shared.state().remember(
                text_id,
                Asked {
                    run: run.clone(),
                    arguments: arguments_digest(&asked.arguments),
                    sent: kept_arguments(&asked.arguments),
                    action: asked.action,
                },
            );
            Response::json(201, &json!({"run": run.as_str(), "state": "running"}))
        }
        Ok(Answer::Repeat(earlier)) => Response::json(
            200,
            &json!({"run": earlier.run().as_str(), "repeat": true,
                    "state": state_word(earlier.state(), false)}),
        ),
        Ok(Answer::Ran(finished)) => Response::json(
            200,
            &json!({"run": finished.run().as_str(), "state": "ended",
                    "outcome": finished.outcome().name()}),
        ),
        Err(error) => call_refused(&error),
    }
}

fn state_word(state: &RunState, stopping: bool) -> &'static str {
    match state {
        RunState::NotEnded { here: true } if stopping => "stopping",
        RunState::NotEnded { here: true } => "running",
        RunState::NotEnded { here: false } => "not_here",
        RunState::Abandoned { .. } => "abandoned",
        RunState::Ended(_) => "ended",
        RunState::Removed { .. } => "removed",
    }
}

/// A typed event's texts, escaped as a message is: every string in it.
fn escaped_event(event: &Json) -> Json {
    match event {
        Json::String(text) => json!(escaped(text)),
        Json::Object(fields) => Json::Object(
            fields
                .iter()
                .map(|(name, value)| (name.clone(), escaped_event(value)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn watched_json(watched: &Watched, stopping: bool) -> Json {
    let events: Vec<Json> = watched
        .events()
        .iter()
        .map(|line| {
            let mut event = serde_json::to_value(line).unwrap_or(Json::Null);
            event["message"] = json!(escaped(line.message()));
            if let Some(typed) = event.get("event").cloned() {
                event["event"] = escaped_event(&typed);
            }
            event
        })
        .collect();
    let mut said = json!({
        "run": watched.run().as_str(),
        "state": state_word(watched.state(), stopping),
        "events": events,
        "next": watched.next(),
        "skipped": watched.skipped(),
    });
    if let RunState::Ended(outcome) = watched.state() {
        said["outcome"] = json!(outcome.name());
        let outputs: Vec<Json> = outcome
            .outputs()
            .iter()
            .map(|committed| {
                json!({"port": committed.port(), "type": committed.artifact().name(),
                       "id": committed.id().to_string()})
            })
            .collect();
        said["outputs"] = Json::Array(outputs);
    }
    said
}

/// The events of an ended run's journal from the place `after` on, escaped; the place
/// after its last event; and how many places from `after` on hold an event the run did
/// not keep: the gaps between the kept events' places, and those it counted after its
/// last kept one (a sequence number is given to every event, kept or not).
fn from_journal(lines: &[Json], not_kept: u64, after: u64) -> (Vec<Json>, u64, u64) {
    let seqs: Vec<u64> = lines
        .iter()
        .filter_map(|line| line["seq"].as_u64())
        .collect();
    // Every gap from the first place on, and so how many were not kept after the last.
    let mut gaps_all = 0u64;
    let mut expected = 0u64;
    for seq in &seqs {
        gaps_all += seq.saturating_sub(expected);
        expected = seq + 1;
    }
    let trailing = not_kept.saturating_sub(gaps_all);
    let end = expected + trailing;
    let mut skipped = 0u64;
    let mut expected = after;
    let mut events = Vec::new();
    for (line, seq) in lines.iter().zip(&seqs) {
        if *seq < after {
            continue;
        }
        skipped += seq - expected;
        expected = seq + 1;
        events.push(escaped_event(line));
    }
    skipped += end.saturating_sub(expected.max(after));
    (events, end.max(after), skipped)
}

fn run_of(guarded: &Guarded<'_>) -> Result<RunId, Response> {
    let text = guarded
        .params
        .first()
        .map(String::as_str)
        .unwrap_or_default();
    RunId::parse(text).map_err(|_| Response::refused(404, "no run has that id"))
}

fn status(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    let run = match run_of(guarded) {
        Ok(run) => run,
        Err(refused) => return refused,
    };
    let after = match guarded.head.field("after") {
        None => 0,
        Some(text) => match text.parse::<u64>() {
            Ok(after) => after,
            Err(_) => return Response::refused(400, "after is a number"),
        },
    };
    let (stopping, asked) = {
        let state = shared.state();
        let held = state
            .runs
            .iter()
            .find(|held| held.run.as_ref() == Some(&run));
        let asked = held
            .map(|held| (held.action.clone(), held.arguments.clone()))
            .or_else(|| {
                // A run that has ended is no longer held: what was asked under its
                // request id says what it was.
                let earlier = state.requests.values().find(|asked| asked.run == run)?;
                Some((earlier.action.clone(), earlier.sent.clone()))
            });
        (held.is_some_and(|held| held.stopping), asked)
    };
    let Some(executor) = shared.executor() else {
        return Response::refused(503, "the playground is stopping");
    };
    let watched = match executor.status_after(&run, after) {
        Ok(watched) => watched,
        Err(error) => return call_refused(&error),
    };
    drop(executor);
    let mut said = watched_json(&watched, stopping);
    // A run that has ended: the events the page has not seen are in its journal.
    if matches!(watched.state(), RunState::Ended(_))
        && watched.events().is_empty()
        && let Some((lines, not_kept)) = shared.play.journal(&run)
    {
        let (events, next, skipped) = from_journal(&lines, not_kept, after);
        said["events"] = Json::Array(events);
        said["next"] = json!(next);
        said["skipped"] = json!(skipped);
    }
    let (action, arguments) = asked.map_or((Json::Null, Json::Null), |(action, arguments)| {
        (json!(action), arguments)
    });
    said["action"] = action;
    said["arguments"] = arguments;
    let ended = matches!(watched.state(), RunState::Ended(_));
    for (name, value) in shared.play.facts(&run, ended) {
        said[name] = value;
    }
    Response::json(200, &said)
}

fn stop(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    let run = match run_of(guarded) {
        Ok(run) => run,
        Err(refused) => return refused,
    };
    match shared.stop(&run) {
        true => Response::json(202, &json!({"run": run.as_str(), "state": "stopping"})),
        false => Response::refused(
            404,
            "this playground started no run of that id that is going",
        ),
    }
}

pub(crate) fn open(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    if guarded.head.field("open") != Some("1") {
        return Response::refused(
            400,
            "this build lists the runs still going: ask with open=1",
        );
    }
    // A run that has ended is not listed, whether or not the keeper has looked yet.
    shared.forget_ended();
    let state = shared.state();
    let runs: Vec<Json> = state
        .runs
        .iter()
        .filter_map(|held| {
            let run = held.run.as_ref()?;
            Some(json!({
                "run": run.as_str(),
                "action": held.action,
                "started_ms": held.started_ms,
                "state": if held.stopping { "stopping" } else { "running" },
            }))
        })
        .collect();
    Response::json(200, &json!({"runs": runs}))
}

/// The most bytes of a call's arguments the host keeps to show again with its run.
const ARGUMENTS_KEPT: usize = 16 * 1024;

/// A call's arguments as kept beside its request id: as sent, or none when they are
/// longer than [`ARGUMENTS_KEPT`].
fn kept_arguments(arguments: &Map<String, Json>) -> Json {
    let whole = Json::Object(arguments.clone());
    match whole.to_string().len() <= ARGUMENTS_KEPT {
        true => whole,
        false => Json::Null,
    }
}

fn refused_by_play(refused: &super::Refused) -> Response {
    Response::refused(refused.status, &escaped(&refused.why))
}

fn picture(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    let name = guarded.head.field("name").unwrap_or_default();
    if name.chars().count() > 200 {
        return Response::refused(400, "a picture's name is at most 200 characters");
    }
    let bytes = guarded
        .picture
        .as_ref()
        .map_or(&[][..], |picture| &picture.bytes[..]);
    match shared.play.picture(bytes, name) {
        Ok(made) => Response::json(201, &made),
        Err(refused) => refused_by_play(&refused),
    }
}

/// The most pictures one page of the list holds.
const PICTURES_PAGE: usize = 200;

fn pictures(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    let limit = match guarded.head.field("limit") {
        None => 60,
        Some(text) => match text.parse::<usize>() {
            Ok(limit) if (1..=PICTURES_PAGE).contains(&limit) => limit,
            _ => return Response::refused(400, "limit is a number from 1 to 200"),
        },
    };
    match shared.play.pictures(guarded.head.field("after"), limit) {
        Ok(listed) => Response::json(200, &listed),
        Err(refused) => refused_by_play(&refused),
    }
}

/// A place among the pictures being made for display, of one kind (thumbnails, or
/// artifacts whole), given back when dropped.
struct Making<'a> {
    shared: &'a Shared,
    thumbnail: bool,
}

impl Drop for Making<'_> {
    fn drop(&mut self) {
        let mut state = self.shared.state();
        let made = match self.thumbnail {
            true => &mut state.thumbnails,
            false => &mut state.displays,
        };
        *made = made.saturating_sub(1);
    }
}

fn artifact_response(shared: &Shared, guarded: &Guarded<'_>, edge: Option<u32>) -> Response {
    // At most two of each kind made at once, as uploads are: a page of pictures leaves
    // the workers that polls, Stop and Quit need. The page retries a 429 after its
    // `Retry-After`.
    let thumbnail = edge.is_some();
    {
        let mut state = shared.state();
        let made = match thumbnail {
            true => &mut state.thumbnails,
            false => &mut state.displays,
        };
        if *made >= super::DISPLAYS_LIMIT {
            let why = match thumbnail {
                true => "at most two thumbnails are made at once",
                false => "at most two artifacts are shown whole at once",
            };
            return Response::refused(429, why).with("Retry-After", "1");
        }
        *made += 1;
    }
    let _making = Making { shared, thumbnail };
    let id = guarded
        .params
        .first()
        .map(String::as_str)
        .unwrap_or_default();
    match shared.play.artifact(id, edge) {
        Ok((content_type, bytes)) => Response::bytes(200, content_type, bytes),
        Err(refused) => refused_by_play(&refused),
    }
}

fn artifact(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    artifact_response(shared, guarded, None)
}

/// The longest side a thumbnail may be asked for.
const THUMB_EDGE_LIMIT: u32 = 2048;

fn thumb(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    let edge = match guarded.head.field("edge") {
        None => 256,
        Some(text) => match text.parse::<u32>() {
            Ok(edge) if (8..=THUMB_EDGE_LIMIT).contains(&edge) => edge,
            _ => return Response::refused(400, "edge is a number of pixels from 8 to 2048"),
        },
    };
    artifact_response(shared, guarded, Some(edge))
}

fn location(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    let drawn = guarded.json.clone().unwrap_or(Json::Null);
    match shared.play.location(&drawn) {
        Ok(made) => Response::json(201, &made),
        Err(refused) => refused_by_play(&refused),
    }
}

fn asset(_: &Shared, guarded: &Guarded<'_>) -> Response {
    let path = guarded.head.path.trim_start_matches('/');
    let path = if path.is_empty() { assets::INDEX } else { path };
    match assets::find(path) {
        Some((content_type, bytes)) => {
            let response = Response::bytes(200, content_type, bytes.to_vec());
            match content_type.starts_with("text/html") {
                true => response
                    .with("Content-Security-Policy", PAGE_POLICY)
                    .with("X-Frame-Options", "DENY"),
                false => response,
            }
        }
        None => Response::refused(404, "no such file"),
    }
}
