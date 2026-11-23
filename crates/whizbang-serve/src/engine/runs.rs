//! A run by its id (ADR 0045, "Plain routes"), on the run routes' own handlers:
//! `GET /v1/runs/<run id>`, its state, and once it has ended its outcome, its outputs and
//! its record; and `POST /v1/runs/<run id>/cancel`, which trips the run's cancellation and
//! hands it to the canceller, and never waits.

use std::sync::Arc;

use serde_json::{Value as Json, json};
use whizbang_core::contract::{CallError, Refusal, RunId, RunState};

use super::Shared;
use super::guard::{Asked, Guarded, deadlines};
use crate::http::Response;

pub(crate) fn serve(shared: &Arc<Shared>, guarded: Guarded) {
    let Guarded {
        mut conn, asked, ..
    } = guarded;
    let response = match asked {
        Asked::Status(id) => status(shared, &id),
        Asked::Cancel(id) => cancel(shared, &id),
        _ => return,
    };
    response.send(&mut conn, &deadlines(shared));
}

fn no_run() -> Response {
    Response::refused(404, "no run has that id")
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

/// A run's state as the run routes say it: its id and state, and once it has ended its
/// outcome, its outputs' identities, and its record when the store holds one.
fn said(shared: &Shared, run: &RunId, state: &RunState) -> Json {
    let mut said = json!({
        "run": run.as_str(),
        "state": state_word(state, shared.stopping(run)),
    });
    if let RunState::Ended(outcome) = state {
        said["outcome"] = json!(outcome.name());
        let outputs: Vec<Json> = (outcome.outputs().iter())
            .map(|committed| {
                json!({"port": committed.port(), "type": committed.artifact().name(),
                       "id": committed.id().to_string()})
            })
            .collect();
        said["outputs"] = Json::Array(outputs);
        if let Some(record) = shared.hosted.record(run) {
            said["record"] = record;
        }
    }
    said
}

fn state_of(shared: &Shared, run: &RunId) -> Result<RunState, Response> {
    let Some(executor) = shared.executor() else {
        return Err(Response::refused(503, "the engine is stopping"));
    };
    match executor.status_after(run, u64::MAX) {
        Ok(watched) => Ok(watched.state().clone()),
        Err(CallError::Refused(Refusal::NoSuchRun { .. })) => Err(no_run()),
        Err(_) => Err(Response::refused(500, "the store could not answer")),
    }
}

fn status(shared: &Shared, id: &str) -> Response {
    let Ok(run) = RunId::parse(id) else {
        return no_run();
    };
    match state_of(shared, &run) {
        Ok(state) => Response::json(200, &said(shared, &run, &state)),
        Err(refused) => refused,
    }
}

fn cancel(shared: &Shared, id: &str) -> Response {
    let Ok(run) = RunId::parse(id) else {
        return no_run();
    };
    // Its status first, which answers during a final publication: a run that has ended
    // is answered with its end, though its handler has not let it go yet.
    let state = match state_of(shared, &run) {
        Ok(state) => state,
        Err(refused) => return refused,
    };
    // A run going here: its cancellation tripped, and its cancel handed on. Never waits.
    if matches!(state, RunState::NotEnded { here: true }) && shared.stop(&run) {
        return Response::json(202, &json!({"run": run.as_str(), "state": "stopping"}));
    }
    // A run that has ended is answered with its end, whoever started it: a cancel of it
    // changes nothing, and a run this server started and no longer remembers is so
    // answered from the store.
    if matches!(state, RunState::Ended(_)) {
        return Response::json(200, &said(shared, &run, &state));
    }
    if !shared.started_here(&run) {
        return Response::refused(
            409,
            "this server did not start that run: one process owns every run it starts",
        );
    }
    Response::json(200, &said(shared, &run, &state))
}
