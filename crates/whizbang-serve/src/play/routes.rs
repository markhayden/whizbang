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
    Answer, CallError, Cancellation, Refusal, Request, RunId, RunState, Watched,
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
        query: &["open"],
        handler: open,
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
        pattern: "*",
        body: Body::None,
        query: &[],
        handler: asset,
    },
];

/// The route of this method and path, with the parts of the path its pattern names: a
/// path no route has is 404, and one whose route takes another method 405.
pub(crate) fn find(method: &str, path: &str) -> Result<(&'static Route, Vec<String>), Unread> {
    let mut path_known = false;
    for route in ROUTES {
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
            let response = (route.handler)(shared, &guarded);
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
                &json!({"refused": escaped(&refusal.to_string()), "kind": kind_word(refusal)}),
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
    match executor.prepare(&call) {
        Ok(prepared) => {
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
            Response::json(200, &json!({"handle": handle, "ready": true}))
        }
        Err(CallError::Refused(refusal)) => Response::json(
            200,
            &json!({"ready": false, "kind": kind_word(&refusal),
                    "why": escaped(&refusal.to_string())}),
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
                &json!({"refused": "another call was started under this request id",
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
    // A place among the calls, held while the start is admitted, and given back if it
    // makes no run.
    let signal = Cancellation::new();
    let until = Instant::now() + shared.settings.run_limit;
    let Some(admitting) = shared.admit(&asked.action, &signal, until, now_ms()) else {
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

fn watched_json(watched: &Watched, stopping: bool) -> Json {
    let events: Vec<Json> = watched
        .events()
        .iter()
        .map(|line| {
            let mut event = serde_json::to_value(line).unwrap_or(Json::Null);
            event["message"] = json!(escaped(line.message()));
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
    }
    said
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
    let stopping = shared
        .state()
        .runs
        .iter()
        .any(|held| held.run.as_ref() == Some(&run) && held.stopping);
    let Some(executor) = shared.executor() else {
        return Response::refused(503, "the playground is stopping");
    };
    match executor.status_after(&run, after) {
        Ok(watched) => Response::json(200, &watched_json(&watched, stopping)),
        Err(error) => call_refused(&error),
    }
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

fn open(shared: &Shared, guarded: &Guarded<'_>) -> Response {
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
        Err(why) => Response::refused(501, &escaped(&why)),
    }
}

fn asset(_: &Shared, guarded: &Guarded<'_>) -> Response {
    let path = guarded.head.path.trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
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
