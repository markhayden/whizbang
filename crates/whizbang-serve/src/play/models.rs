//! The Models screen's routes (plan m5, task M1; ADR 0041, "Routes"): the catalogue's
//! models with what is installed and an install's progress, Get, Cancel and Remove; each
//! backend the config offers and whether its key is set, never the key; and the
//! allowance, with the owner's reset.
//!
//! **An install is never on a connection worker.** `POST /api/models/{name}/get` checks
//! what it can at once, then starts the install on a thread of the host's own counted
//! spawner ([`super::threads`]) and answers 202. The host runs one install at a time: a
//! second Get of the same model attaches to it (answered 200, `attached`), and a Get of
//! another model is refused while one goes. The install carries a [`Cancellation`] the
//! host keeps: Cancel trips it, and so does a quit, at the start of its grace, which then
//! waits for the install as it waits for the runs ([`Installs::idle`]). A cancelled
//! install leaves the installation that was there before it as it was (ADR 0017).
//!
//! **A removal** waits for the runs that hold the model for the short wait the door
//! gives it, then answers busy: the only request of this module that holds a worker for
//! longer than a read, one at a time (a second is answered 429).
//!
//! **Nothing here holds a key.** What the door answers is passed on as it is; the door
//! says whether a key is set, never the key or the name of its variable, and a test
//! scans every answer for the env file's values.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

use serde::Deserialize;
use serde_json::{Value as Json, json};
use whizbang_core::contract::Cancellation;

use super::guard::Guarded;
use super::http::Response;
use super::routes::{Body, Route};
use super::{Refused, Shared};
use crate::escaped;

/// What the binary hands the host for the Models screen: the models directory and the
/// catalogue, the config's backends, and the store's books. Every answer is JSON the
/// page reads as it is, with every text from the config, the catalogue or the store
/// escaped, and nothing of a key, a sign-in or a token.
pub trait Models: Send + Sync {
    /// `{"models": [...]}`: each model of the catalogue, with `name`, `title`,
    /// `licence`, `bytes`, `files`, `state` (a word) and `why`, and, while an install
    /// of it runs, `files_done`.
    fn models(&self) -> Result<Json, Refused>;
    /// `{"backends": [...]}`: each backend the config offers, by capability, with
    /// whether its key is set, and its price as the backend states it.
    fn backends(&self) -> Result<Json, Refused>;
    /// The allowance of the newest epoch, what is spent and held since the reset that
    /// opened it and when that was, and the config's cap of one call.
    fn spend(&self) -> Result<Json, Refused>;
    /// `spend reset`, to this many US dollars as typed: what was opened.
    fn reset(&self, allowance: &str) -> Result<Json, Refused>;
    /// Whether an install of this model may start now, asked on the connection worker
    /// before the install's thread is made: the catalogue holds it, it runs here, this
    /// build has its adapter, and its download's sign-in is named where it needs one.
    fn installable(&self, model: &str) -> Result<(), Refused>;
    /// Installs the model, or attaches to an install of it another process runs: on
    /// the install's own thread, until it ends or `cancellation` is tripped. What was
    /// installed, or why not, in words that hold no secret.
    fn install(&self, model: &str, cancellation: &Cancellation) -> Result<String, NotInstalled>;
    /// Removes the model, waiting for the runs that hold it for the door's short wait;
    /// past it, busy (409), and the model is as it was.
    fn remove(&self, model: &str) -> Result<Json, Refused>;
}

/// The routes of this module, registered in [`super::routes`] by one line.
pub(crate) const ROUTES: &[Route] = &[
    Route {
        method: "GET",
        pattern: "/api/models",
        body: Body::None,
        query: &[],
        handler: list,
    },
    Route {
        method: "POST",
        pattern: "/api/models/{id}/get",
        body: Body::None,
        query: &[],
        handler: get,
    },
    Route {
        method: "POST",
        pattern: "/api/models/{id}/cancel",
        body: Body::None,
        query: &[],
        handler: cancel,
    },
    Route {
        method: "DELETE",
        pattern: "/api/models/{id}",
        body: Body::None,
        query: &[],
        handler: remove,
    },
    Route {
        method: "GET",
        pattern: "/api/backends",
        body: Body::None,
        query: &[],
        handler: backends,
    },
    Route {
        method: "GET",
        pattern: "/api/spend",
        body: Body::None,
        query: &[],
        handler: spend,
    },
    Route {
        method: "POST",
        pattern: "/api/spend/reset",
        body: Body::Json,
        query: &[],
        handler: reset,
    },
];

/// Why an install the host ran did not install its model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotInstalled {
    /// The host's request had attached to an install another process runs, and left it
    /// when it was cancelled: that install goes on, there.
    pub left: bool,
    /// Why, in words that hold no secret.
    pub why: String,
}

/// How an install the host ran ended, as the page is told it beside the model's row.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Ended {
    /// `installed`, `cancelled`, `detached` (the host stopped following an install
    /// another process runs, which goes on) or `failed`.
    word: &'static str,
    why: Option<String>,
}

/// The install the host runs, if any.
struct Going {
    model: String,
    signal: Cancellation,
}

/// What the install's thread tells when it ends: the state to change, and whom to wake.
#[derive(Default)]
struct Waking(std::sync::OnceLock<std::sync::Weak<Shared>>);

#[derive(Default)]
struct InstallState {
    going: Option<Going>,
    /// How the last install of each model the host ran ended.
    ended: BTreeMap<String, Ended>,
    /// A quit came: no install starts any more.
    closed: bool,
    /// A removal is waiting for its drain.
    removing: bool,
    /// The thread of the last install, which a new install waits for before it is made,
    /// so that the host never holds two install threads at once.
    thread: Option<std::thread::JoinHandle<()>>,
}

/// The host's one install at a time, and its one removal at a time.
#[derive(Clone, Default)]
pub(crate) struct Installs(Arc<Mutex<InstallState>>, Arc<Waking>);

impl Installs {
    fn state(&self) -> MutexGuard<'_, InstallState> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The host these installs are of, woken when an install ends, so that a quit's
    /// grace sees it at once (set once, when the host starts).
    pub fn wakes(&self, shared: &Arc<Shared>) {
        let _ = self.1.0.set(Arc::downgrade(shared));
    }

    /// A quit: the install going is cancelled, and none starts after.
    pub fn close(&self) {
        let mut state = self.state();
        state.closed = true;
        if let Some(going) = &state.going {
            going.signal.cancel();
        }
    }

    /// No install is going.
    pub fn idle(&self) -> bool {
        self.state().going.is_none()
    }

    fn finished(&self, model: &str, outcome: Result<String, NotInstalled>, cancelled: bool) {
        let mut state = self.state();
        state.going = None;
        let ended = match outcome {
            Ok(_) => Ended {
                word: "installed",
                why: None,
            },
            Err(NotInstalled { left: true, .. }) => Ended {
                word: "detached",
                why: Some(
                    "the playground stopped following the install another process runs; it \
                     goes on there"
                        .to_owned(),
                ),
            },
            Err(_) if cancelled => Ended {
                word: "cancelled",
                why: Some(
                    "the install was cancelled; what was installed before is as it was".to_owned(),
                ),
            },
            Err(NotInstalled { why, .. }) => Ended {
                word: "failed",
                why: Some(escaped(&why)),
            },
        };
        state.ended.insert(model.to_owned(), ended);
        drop(state);
        if let Some(shared) = self.1.0.get().and_then(std::sync::Weak::upgrade) {
            shared.notify();
        }
    }
}

/// Lets the removal's place go however the handler ends.
struct Removing<'a>(&'a Installs);

impl Drop for Removing<'_> {
    fn drop(&mut self) {
        self.0.state().removing = false;
    }
}

fn models_of(shared: &Shared) -> Result<&dyn Models, Response> {
    shared
        .play
        .models()
        .ok_or_else(|| Response::refused(501, "this build lists no models in the playground"))
}

fn refused(refused: &Refused) -> Response {
    Response::refused(refused.status, &refused.why)
}

fn answer(said: Result<Json, Refused>, status: u16) -> Response {
    match said {
        Ok(said) => Response::json(status, &said),
        Err(why) => refused(&why),
    }
}

/// The model a route names: a name of the catalogue's form, lower-case letters, digits,
/// `-`, `_` and `.`; anything else names no model.
fn model_of(guarded: &Guarded<'_>) -> Result<String, Response> {
    let name = guarded.params.first().cloned().unwrap_or_default();
    let fits = !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-_.".contains(&b));
    match fits {
        true => Ok(name),
        false => Err(Response::refused(
            404,
            "the catalogue holds no model of that name",
        )),
    }
}

fn list(shared: &Shared, _: &Guarded<'_>) -> Response {
    let models = match models_of(shared) {
        Ok(models) => models,
        Err(response) => return response,
    };
    let mut said = match models.models() {
        Ok(said) => said,
        Err(why) => return refused(&why),
    };
    let state = shared.installs.state();
    if let Some(rows) = said.get_mut("models").and_then(Json::as_array_mut) {
        for row in rows {
            let Some(name) = row.get("name").and_then(Json::as_str).map(str::to_owned) else {
                continue;
            };
            let here = state.going.as_ref().filter(|going| going.model == name);
            if let Some(going) = here {
                row["install"] = json!(if going.signal.is_cancelled() {
                    "stopping"
                } else {
                    "installing"
                });
            } else if let Some(ended) = state.ended.get(&name) {
                row["last"] = json!({"ended": ended.word, "why": ended.why});
            }
        }
    }
    said["installing"] = json!(state.going.as_ref().map(|going| going.model.clone()));
    Response::json(200, &said)
}

/// What a Get of `model` is answered with as things stand, when it is not to start an
/// install: the host is stopping; an install of it goes (attached) or is stopping; another
/// model's install goes; a removal goes.
fn not_now(state: &InstallState, model: &str) -> Option<Response> {
    if state.closed {
        return Some(Response::refused(503, "the playground is stopping"));
    }
    match &state.going {
        Some(going) if going.model == model && going.signal.is_cancelled() => Some(
            Response::refused(409, "the install is stopping; try again after it"),
        ),
        Some(going) if going.model == model => Some(Response::json(
            200,
            &json!({"model": model, "state": "installing", "attached": true}),
        )),
        Some(_) => Some(Response::refused(
            409,
            "another model is being installed; the playground installs one at a time",
        )),
        None if state.removing => Some(Response::refused(
            409,
            "a model is being removed; try again after it",
        )),
        None => None,
    }
}

fn get(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    let model = match model_of(guarded) {
        Ok(model) => model,
        Err(response) => return response,
    };
    let models = match models_of(shared) {
        Ok(models) => models,
        Err(response) => return response,
    };
    let installs = shared.installs.clone();
    if let Some(answer) = not_now(&installs.state(), &model) {
        return answer;
    }
    if let Err(why) = models.installable(&model) {
        return refused(&why);
    }
    // The checks again under the lock, now that the door has looked: another Get, or a
    // removal, may have started meanwhile.
    let mut state = installs.state();
    if let Some(answer) = not_now(&state, &model) {
        return answer;
    }
    // The last install's thread has said it ended; it is let finish before another is
    // made, so the host never holds two (it is past its work, so this is a moment).
    if let Some(ended) = state.thread.take() {
        let _ = ended.join();
    }
    let signal = Cancellation::new();
    state.going = Some(Going {
        model: model.clone(),
        signal: signal.clone(),
    });
    state.ended.remove(&model);
    drop(state);
    let play = shared.play.clone();
    let (ending, named) = (installs.clone(), model.clone());
    let made = super::threads::spawn(
        "install",
        Box::new(move || {
            let outcome =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match play.models() {
                    Some(models) => models.install(&named, &signal),
                    None => Err(NotInstalled {
                        left: false,
                        why: "this build installs no model from the playground".to_owned(),
                    }),
                }))
                .unwrap_or_else(|_| {
                    Err(NotInstalled {
                        left: false,
                        why: "the install stopped on a fault of whizbang's".to_owned(),
                    })
                });
            ending.finished(&named, outcome, signal.is_cancelled());
        }),
    );
    match made {
        Ok(thread) => {
            installs.state().thread = Some(thread);
            Response::json(
                202,
                &json!({"model": model, "state": "installing", "attached": false}),
            )
        }
        Err(_) => {
            installs.state().going = None;
            Response::refused(503, "the host cannot start an install now; try again")
                .with("Retry-After", "1")
        }
    }
}

fn cancel(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    let model = match model_of(guarded) {
        Ok(model) => model,
        Err(response) => return response,
    };
    let state = shared.installs.state();
    match &state.going {
        Some(going) if going.model == model => {
            going.signal.cancel();
            Response::json(202, &json!({"model": model, "state": "stopping"}))
        }
        _ => Response::refused(404, "the playground is installing no model of that name"),
    }
}

fn remove(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    let model = match model_of(guarded) {
        Ok(model) => model,
        Err(response) => return response,
    };
    let models = match models_of(shared) {
        Ok(models) => models,
        Err(response) => return response,
    };
    let installs = &shared.installs;
    {
        let mut state = installs.state();
        if state
            .going
            .as_ref()
            .is_some_and(|going| going.model == model)
        {
            return Response::refused(
                409,
                "the model is being installed; cancel the install before removing it",
            );
        }
        if state.removing {
            return Response::refused(429, "one model is removed at a time")
                .with("Retry-After", "1");
        }
        state.removing = true;
    }
    let _removing = Removing(installs);
    let said = models.remove(&model);
    if said.is_ok() {
        installs.state().ended.remove(&model);
    }
    answer(said, 200)
}

fn backends(shared: &Shared, _: &Guarded<'_>) -> Response {
    match models_of(shared) {
        Ok(models) => answer(models.backends(), 200),
        Err(response) => response,
    }
}

fn spend(shared: &Shared, _: &Guarded<'_>) -> Response {
    match models_of(shared) {
        Ok(models) => answer(models.spend(), 200),
        Err(response) => response,
    }
}

/// The body of a reset: the new allowance as typed, and the word `reset` typed again.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Resetting {
    allowance: String,
    confirm: String,
}

fn reset(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    let models = match models_of(shared) {
        Ok(models) => models,
        Err(response) => return response,
    };
    let json = guarded.json.clone().unwrap_or(Json::Null);
    let Ok(asked) = serde_json::from_value::<Resetting>(json) else {
        return Response::refused(
            400,
            "a reset names the new allowance and the word reset, and nothing else",
        );
    };
    if asked.confirm != "reset" {
        return Response::refused(
            400,
            "type the word reset to confirm: a reset closes the books",
        );
    }
    answer(models.reset(&asked.allowance), 200)
}
