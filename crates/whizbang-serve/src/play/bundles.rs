//! Bundles, a chain's steps and the replay of a run (plan m7, task U2; ADR 0041, "Routes",
//! amended; ADR 0048; ADR 0049): the routes, registered by one line in [`super::routes`]'s
//! list of tables, and what the binary answers them with, [`Bundles`]. Every text a
//! person, a model, a tool or a bundle gave is escaped by the binary before it is handed
//! on.
//!
//! - `GET /api/runs/{id}/bundle`: one ended root run's bundle, a download;
//!   `GET /api/runs/{id}/bundle/about`: what it would hold, and about how many bytes.
//! - `POST /api/bundles/chain`: a chain of runs in the order they ran, at most 16, a
//!   download; with `"check": true`, its steps and links alone, or the refusal.
//! - `POST /api/bundles`: a bundle's bytes, at most 256 MiB, read whole by the core's
//!   reader and imported, one bundle at a time; it takes one of the host's two upload
//!   places, as a picture does. No route takes a path.
//! - `GET /api/bundles`, `GET /api/bundles/{digest}`, `DELETE /api/bundles/{digest}`.
//! - `POST /api/bundles/{digest}/steps/{n}/open`: the draft of a chain's step, its linked
//!   inputs bound to the new runs of the earlier steps. It starts nothing: a step runs
//!   only when the person presses Run, through `POST /api/runs` with `replay`.
//! - `POST /api/runs/{id}/replay`: a replay of a run, offline or live, started on the
//!   host's one executor and held among its runs as any run it starts;
//!   `GET /api/runs/{id}/replay`: a replay's report, once it has ended.

use std::time::Instant;

use serde_json::{Value as Json, json};
use whizbang_core::contract::replay::{Replay, Source};
use whizbang_core::contract::{Answer, Cancellation, RunId};

use super::guard::Guarded;
use super::history::{Document, download};
use super::http::Response;
use super::routes::{Body, Route, call_refused, now_ms, refused_by_play, run_of, state_word};
use super::{Asked, Refused, Shared, arguments_digest};

/// The most runs a chain holds.
pub const CHAIN_LIMIT: usize = whizbang_core::bundle::STEPS_LIMIT;

/// What the binary answers bundles and a replay's report with. It holds no HTTP type.
pub trait Bundles: Send + Sync {
    /// The bundle of one ended root run.
    fn bundle_run(&self, run: &RunId) -> Result<Document, Refused>;
    /// What the bundle of one run would hold, and about how many bytes: the sum of what
    /// the store holds of its family, before the bundle is written.
    fn bundle_about(&self, run: &RunId) -> Result<Json, Refused>;
    /// The bundle of a chain of root runs, in the order given, and what it holds: its
    /// steps, their links and what it withholds.
    fn bundle_chain(&self, runs: &[RunId]) -> Result<(Document, Json), Refused>;
    /// A chain's steps and their links, or the refusal the writer would give, without
    /// reading the runs' files or writing the bundle.
    fn check_chain(&self, runs: &[RunId]) -> Result<Json, Refused>;
    /// Reads a bundle's bytes whole and imports it.
    fn import(&self, bytes: &[u8]) -> Result<Json, Refused>;
    /// Every bundle the store holds.
    fn list(&self) -> Result<Json, Refused>;
    /// One bundle the store holds: its steps, links, files and what it withholds, and
    /// whether each step replays, as this build decides it from the step's records.
    fn show(&self, digest: &str) -> Result<Json, Refused>;
    /// Lets go of a bundle; what a run names stays.
    fn delete(&self, digest: &str) -> Result<Json, Refused>;
    /// The draft of a chain's step, its linked inputs bound to the outputs of the runs
    /// `runs` names for the earlier steps (one for each, or none).
    fn open_step(&self, digest: &str, step: u32, runs: &[Option<String>]) -> Result<Json, Refused>;
    /// The report of a replayed run that has ended.
    fn replay_report(&self, run: &RunId) -> Result<Json, Refused>;
}

/// The routes of this module, which the route table lists after its own.
pub(crate) const ROUTES: &[Route] = &[
    Route {
        method: "GET",
        pattern: "/api/runs/{id}/bundle",
        body: Body::None,
        query: &[],
        handler: bundle_run,
    },
    Route {
        method: "GET",
        pattern: "/api/runs/{id}/bundle/about",
        body: Body::None,
        query: &[],
        handler: bundle_about,
    },
    Route {
        method: "POST",
        pattern: "/api/bundles/chain",
        body: Body::Json,
        query: &[],
        handler: chain,
    },
    Route {
        method: "POST",
        pattern: "/api/bundles",
        body: Body::Bundle,
        query: &[],
        handler: import,
    },
    Route {
        method: "GET",
        pattern: "/api/bundles",
        body: Body::None,
        query: &[],
        handler: list,
    },
    Route {
        method: "GET",
        pattern: "/api/bundles/{id}",
        body: Body::None,
        query: &[],
        handler: show,
    },
    Route {
        method: "DELETE",
        pattern: "/api/bundles/{id}",
        body: Body::None,
        query: &[],
        handler: delete,
    },
    Route {
        method: "POST",
        pattern: "/api/bundles/{id}/steps/{id}/open",
        body: Body::Json,
        query: &[],
        handler: open_step,
    },
    Route {
        method: "POST",
        pattern: "/api/runs/{id}/replay",
        body: Body::Json,
        query: &[],
        handler: replay,
    },
    Route {
        method: "GET",
        pattern: "/api/runs/{id}/replay",
        body: Body::None,
        query: &[],
        handler: report,
    },
];

fn bundles(shared: &Shared) -> Result<&dyn Bundles, Response> {
    shared
        .play
        .bundles()
        .ok_or_else(|| Response::refused(501, "this build makes no bundles and replays nothing"))
}

fn answered(said: Result<Json, Refused>, status: u16) -> Response {
    match said {
        Ok(said) => Response::json(status, &said),
        Err(refused) => refused_by_play(&refused),
    }
}

/// A bundle's digest in a path: `sha256:` and 64 lower-case hex digits.
pub fn is_digest(text: &str) -> bool {
    super::saved::is_identity(text)
}

fn digest_of(guarded: &Guarded<'_>) -> Result<String, Response> {
    let text = guarded.params.first().map(String::as_str).unwrap_or("");
    match is_digest(text) {
        true => Ok(text.to_owned()),
        false => Err(Response::refused(
            404,
            "the store holds no bundle of that digest",
        )),
    }
}

/// A place among the host's two uploads, taken by the writing of a bundle, which reads a
/// run's whole family: so polls, Stop and Quit always find a worker (ADR 0041). Given back
/// when dropped.
struct Writing<'a>(&'a Shared);

impl Drop for Writing<'_> {
    fn drop(&mut self) {
        let mut state = self.0.state();
        state.uploads = state.uploads.saturating_sub(1);
    }
}

fn writing(shared: &Shared) -> Result<Writing<'_>, Response> {
    let mut state = shared.state();
    if state.uploads >= super::UPLOADS_LIMIT {
        return Err(Response::refused(
            429,
            "at most two pictures or bundles are sent or written at once",
        )
        .with("Retry-After", "1"));
    }
    state.uploads += 1;
    Ok(Writing(shared))
}

fn bundle_run(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    let run = match run_of(guarded) {
        Ok(run) => run,
        Err(refused) => return refused,
    };
    let _place = match writing(shared) {
        Ok(place) => place,
        Err(refused) => return refused,
    };
    match bundles(shared).map(|bundles| bundles.bundle_run(&run)) {
        Ok(Ok(document)) => download(document),
        Ok(Err(why)) => refused_by_play(&why),
        Err(refused) => refused,
    }
}

fn bundle_about(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    let run = match run_of(guarded) {
        Ok(run) => run,
        Err(refused) => return refused,
    };
    match bundles(shared) {
        Ok(bundles) => answered(bundles.bundle_about(&run), 200),
        Err(refused) => refused,
    }
}

/// The body of a chain: its runs in the order they ran, and whether only to check it.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Chained {
    runs: Vec<String>,
    #[serde(default)]
    check: bool,
}

fn chain(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    let asked: Chained = match super::routes::body(guarded) {
        Ok(asked) => asked,
        Err(refused) => return refused,
    };
    if asked.runs.is_empty() || asked.runs.len() > CHAIN_LIMIT {
        return Response::refused(400, "a chain holds 1 to 16 runs");
    }
    let mut runs = Vec::new();
    for text in &asked.runs {
        match RunId::parse(text) {
            Ok(run) => runs.push(run),
            Err(_) => return Response::refused(404, "no run has that id"),
        }
    }
    let bundles = match bundles(shared) {
        Ok(bundles) => bundles,
        Err(refused) => return refused,
    };
    // A check reads the runs' records alone, and writes nothing.
    if asked.check {
        return answered(bundles.check_chain(&runs), 200);
    }
    let _place = match writing(shared) {
        Ok(place) => place,
        Err(refused) => return refused,
    };
    match bundles.bundle_chain(&runs) {
        Ok((document, _)) => download(document),
        Err(why) => refused_by_play(&why),
    }
}

fn import(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    let bundles = match bundles(shared) {
        Ok(bundles) => bundles,
        Err(refused) => return refused,
    };
    let bytes = guarded.bundle.as_deref().unwrap_or_default();
    // One bundle is read and imported at a time: the reader's memory is one artifact's
    // and the manifest, and a second waits for the first.
    let _one = shared
        .importing
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    answered(bundles.import(bytes), 201)
}

fn list(shared: &Shared, _: &Guarded<'_>) -> Response {
    match bundles(shared) {
        Ok(bundles) => answered(bundles.list(), 200),
        Err(refused) => refused,
    }
}

fn show(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    let digest = match digest_of(guarded) {
        Ok(digest) => digest,
        Err(refused) => return refused,
    };
    match bundles(shared) {
        Ok(bundles) => answered(bundles.show(&digest), 200),
        Err(refused) => refused,
    }
}

fn delete(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    let digest = match digest_of(guarded) {
        Ok(digest) => digest,
        Err(refused) => return refused,
    };
    match bundles(shared) {
        Ok(bundles) => answered(bundles.delete(&digest), 200),
        Err(refused) => refused,
    }
}

/// The body of a step's opening: the new run of each earlier step, or none.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Opening {
    #[serde(default)]
    runs: Vec<Option<String>>,
}

fn open_step(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    let digest = match digest_of(guarded) {
        Ok(digest) => digest,
        Err(refused) => return refused,
    };
    let Some(step) = guarded
        .params
        .get(1)
        .and_then(|text| text.parse::<u32>().ok())
    else {
        return Response::refused(404, "a step is named by its number, from 0");
    };
    let asked: Opening = match super::routes::body(guarded) {
        Ok(asked) => asked,
        Err(refused) => return refused,
    };
    if asked.runs.len() > CHAIN_LIMIT {
        return Response::refused(400, "a chain holds at most 16 steps");
    }
    if (asked.runs.iter().flatten()).any(|run| RunId::parse(run).is_err()) {
        return Response::refused(404, "no run has that id");
    }
    match bundles(shared) {
        Ok(bundles) => answered(bundles.open_step(&digest, step, &asked.runs), 200),
        Err(refused) => refused,
    }
}

/// The body of a replay: live or offline, and the page's id for it.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Replaying {
    #[serde(default)]
    live: bool,
    request_id: String,
    /// Live: the handle of the preparation whose cost the page showed.
    #[serde(default)]
    handle: Option<String>,
}

/// `POST /api/runs/{id}/replay`: a replay started on the host's one executor, as a start
/// is: a repeated id answered first, a place among the runs taken, and the run held with
/// the host's signal and stated time, so it is polled, stopped and counted as any run.
fn replay(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    let run = match run_of(guarded) {
        Ok(run) => run,
        Err(refused) => return refused,
    };
    let asked: Replaying = match super::routes::body(guarded) {
        Ok(asked) => asked,
        Err(refused) => return refused,
    };
    let Some(executor) = shared.executor() else {
        return Response::refused(503, "the playground is stopping");
    };
    let action = "replay";
    let mut sent = serde_json::Map::new();
    sent.insert("run".to_owned(), json!(run.as_str()));
    sent.insert("live".to_owned(), json!(asked.live));
    let earlier = shared
        .state()
        .requests
        .get(&asked.request_id)
        .map(|earlier| {
            let same = earlier.action == action && earlier.arguments == arguments_digest(&sent);
            (earlier.run.clone(), same)
        });
    if let Some((made, same)) = earlier {
        if !same {
            return Response::json(
                409,
                &json!({"error": "another call was started under this request id",
                        "kind": "conflict"}),
            );
        }
        let state = executor
            .status_after(&made, u64::MAX)
            .map_or("running", |watched| state_word(watched.state(), false));
        return Response::json(
            200,
            &json!({"run": made.as_str(), "repeat": true, "state": state}),
        );
    }
    let id = match executor.request_id(&asked.request_id) {
        Ok(id) => id,
        Err(error) => return call_refused(&error),
    };
    // A live replay may spend: it runs only under the cost the page showed, from a
    // preparation the host still keeps, and is held to that preparation's bound.
    let shown_bound = match (asked.live, &asked.handle) {
        (false, _) => None,
        (true, None) => {
            return Response::json(
                409,
                &json!({"error": "a live replay runs only under the cost the page showed: prepare it, then replay",
                        "kind": "stale"}),
            );
        }
        (true, Some(handle)) => {
            let age = shared.settings.preparation_age;
            let now = Instant::now();
            let kept = shared
                .state()
                .preparations
                .iter()
                .find(|kept| &kept.handle == handle && now.duration_since(kept.at) < age)
                .map(|kept| kept.prepared.estimate().micros());
            match kept {
                Some(bound) => Some(bound),
                None => {
                    return Response::json(
                        409,
                        &json!({"error": "the cost the page showed is no longer kept: prepare it again, then replay",
                                "kind": "stale"}),
                    );
                }
            }
        }
    };
    let signal = Cancellation::new();
    let until = Instant::now() + shared.settings.run_limit;
    // What a page that follows the run is shown while it goes: the task and the
    // arguments of the run it replays, as Run again would give them.
    let (shown_action, shown) = match shared.play.record(&run) {
        Ok(said) => (
            said["action"].as_str().unwrap_or(action).to_owned(),
            said["again"]["values"]
                .as_object()
                .cloned()
                .unwrap_or_default(),
        ),
        Err(_) => (action.to_owned(), serde_json::Map::new()),
    };
    let Some(admitting) = shared.admit(&shown_action, &shown, &signal, until, now_ms()) else {
        if shared.stopping.load(std::sync::atomic::Ordering::SeqCst) {
            return Response::refused(503, "the playground is stopping");
        }
        let why = format!(
            "at most {} runs started here go at once; try again when one ends",
            shared.settings.calls
        );
        return Response::refused(429, &why).with("Retry-After", "5");
    };
    let mut replay = Replay::of(Source::Run(run))
        .with_id(id)
        .cancelled_by(signal)
        .until(until + shared.settings.grace);
    if asked.live {
        replay = replay.live();
    }
    if let Some(bound) = shown_bound {
        replay = replay.shown(bound);
    }
    match executor.start_replay(&replay) {
        Ok(Answer::Started(made)) => {
            admitting.made(made.clone());
            shared.state().remember(
                asked.request_id,
                Asked {
                    run: made.clone(),
                    arguments: arguments_digest(&sent),
                    sent: Json::Object(sent),
                    action: action.to_owned(),
                },
            );
            Response::json(201, &json!({"run": made.as_str(), "state": "running"}))
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

fn report(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    let run = match run_of(guarded) {
        Ok(run) => run,
        Err(refused) => return refused,
    };
    match bundles(shared) {
        Ok(bundles) => answered(bundles.replay_report(&run), 200),
        Err(refused) => refused,
    }
}
