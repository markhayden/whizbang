//! The result's routes (plan m5, task W2; ADR 0041, "Routes"): a run's record as the
//! result screen reads it, and the command line that makes its call again. Each is
//! registered in [`super::routes::ROUTES`] by one line, and answers what the [`super::Play`]
//! says, every text of it escaped there.

use super::Shared;
use super::guard::Guarded;
use super::http::Response;
use super::routes::{Body, Route, refused_by_play, run_of};

/// `GET /api/runs/{id}/record`.
pub(crate) const RECORD: Route = Route {
    method: "GET",
    pattern: "/api/runs/{id}/record",
    body: Body::None,
    query: &[],
    handler: record,
};

/// `GET /api/runs/{id}/command`.
pub(crate) const COMMAND: Route = Route {
    method: "GET",
    pattern: "/api/runs/{id}/command",
    body: Body::None,
    query: &[],
    handler: command,
};

fn record(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    let run = match run_of(guarded) {
        Ok(run) => run,
        Err(refused) => return refused,
    };
    match shared.play.record(&run) {
        Ok(said) => Response::json(200, &said),
        Err(refused) => refused_by_play(&refused),
    }
}

fn command(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    let run = match run_of(guarded) {
        Ok(run) => run,
        Err(refused) => return refused,
    };
    match shared.play.command(&run) {
        Ok(said) => Response::json(200, &said),
        Err(refused) => refused_by_play(&refused),
    }
}
