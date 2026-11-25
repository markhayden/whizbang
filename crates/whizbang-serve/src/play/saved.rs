//! Saved-run comparisons, saved examples and Home's library (plan m7, task U1; ADR 0041,
//! "Routes", amended): the routes, registered by one line in [`super::routes`]'s list of
//! tables, and what the binary answers them with, [`Saved`]. Every text a person, a model
//! or a tool gave is escaped by the binary before it is handed on.
//!
//! - `GET /api/compare/{baseline}/{attempt}`: two ended root runs of one task, side by
//!   side, from their families: 404 for a run the store does not hold, 409 for one that
//!   has not ended, 422 for two tasks or a child run.
//! - `GET /api/examples`, `POST /api/examples` (a run's id, a name and notes),
//!   `PATCH /api/examples/{id}` (a name, notes, or both), `DELETE /api/examples/{id}`.
//! - `POST /api/examples/{id}/open`: the draft an example opens. It runs nothing: it
//!   gives back an input the store let go of from the example's own names, and prepares
//!   each backend the example chose, naming those this config does not offer now by the
//!   reason preparation gives, which the draft then leaves to the config's default.
//! - `GET /api/library?show=&after=&limit=` and `GET /api/library/{id}`: every picture the
//!   store holds as an input of a run or an import, whatever door gave it, by pages, and
//!   one picture's card with the runs that took it.
//!
//! No route takes a path: a run, an example and a picture are named by their ids.

use serde_json::{Map, Value as Json, json};
use whizbang_core::contract::RunId;

use super::guard::Guarded;
use super::http::Response;
use super::routes::{Body, Route, base_refusal, choice_json, own_readiness, refused_by_play};
use super::{Refused, Shared};
use crate::escaped;

/// The most examples a store holds.
pub const EXAMPLES_LIMIT: usize = 1000;
/// The most characters of an example's name, and of its notes.
pub const NAME_LIMIT: usize = 80;
pub const NOTES_LIMIT: usize = 2000;
/// The most pictures one page of the library holds, and how many when the page does not
/// say.
pub const LIBRARY_PAGE_LIMIT: usize = 200;
pub const LIBRARY_PAGE_DEFAULT: usize = 24;

/// Which pictures of the library a page asks for: Home's chips.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Show {
    /// Every picture the store holds as an input of a run or an import.
    All,
    /// Those a person gave a name, in the playground.
    Named,
    /// Those a run took as an input.
    Inputs,
    /// Those an import brought in.
    Imported,
}

impl Show {
    fn of(text: Option<&str>) -> Option<Show> {
        match text.unwrap_or("all") {
            "all" => Some(Show::All),
            "named" => Some(Show::Named),
            "inputs" => Some(Show::Inputs),
            "imported" => Some(Show::Imported),
            _ => None,
        }
    }
}

/// What the binary answers comparisons, examples and the library with. It holds no HTTP
/// type.
pub trait Saved: Send + Sync {
    /// Two ended root runs of one task, side by side.
    fn compare(&self, baseline: &RunId, attempt: &RunId) -> Result<Json, Refused>;
    /// Every example the store holds: `{"examples": [...], "limit": 1000}`.
    fn examples(&self) -> Result<Json, Refused>;
    /// Saves the run `run` as an example under this name and these notes.
    fn save_example(&self, run: &RunId, name: &str, notes: &str) -> Result<Json, Refused>;
    /// Gives an example another name, other notes, or both.
    fn rename_example(
        &self,
        id: &str,
        name: Option<&str>,
        notes: Option<&str>,
    ) -> Result<Json, Refused>;
    /// Lets go of an example; its run and pictures stay.
    fn delete_example(&self, id: &str) -> Result<Json, Refused>;
    /// The draft an example opens, as `Run again` gives one, with the example's choices
    /// of backend apart (`choices`: each argument, its capability and the backend
    /// chosen); an input the store let go of is given back from the example's names.
    fn open_example(&self, id: &str) -> Result<Json, Refused>;
    /// One page of the library: `{"pictures": [...], "more": bool, "counts": {...}}`.
    fn library(&self, show: Show, after: Option<&str>, limit: usize) -> Result<Json, Refused>;
    /// One picture of the library, with the runs that took it.
    fn library_card(&self, id: &str) -> Result<Json, Refused>;
}

/// The routes of this module, which the route table lists after its own.
pub(crate) const ROUTES: &[Route] = &[
    Route {
        method: "GET",
        pattern: "/api/compare/{id}/{id}",
        body: Body::None,
        query: &[],
        handler: compare,
    },
    Route {
        method: "GET",
        pattern: "/api/examples",
        body: Body::None,
        query: &[],
        handler: examples,
    },
    Route {
        method: "POST",
        pattern: "/api/examples",
        body: Body::Json,
        query: &[],
        handler: save,
    },
    Route {
        method: "PATCH",
        pattern: "/api/examples/{id}",
        body: Body::Json,
        query: &[],
        handler: rename,
    },
    Route {
        method: "DELETE",
        pattern: "/api/examples/{id}",
        body: Body::None,
        query: &[],
        handler: delete,
    },
    Route {
        method: "POST",
        pattern: "/api/examples/{id}/open",
        body: Body::None,
        query: &[],
        handler: open,
    },
    Route {
        method: "GET",
        pattern: "/api/library",
        body: Body::None,
        query: &["show", "after", "limit"],
        handler: library,
    },
    Route {
        method: "GET",
        pattern: "/api/library/{id}",
        body: Body::None,
        query: &[],
        handler: card,
    },
];

fn saved(shared: &Shared) -> Result<&dyn Saved, Response> {
    shared
        .play
        .saved()
        .ok_or_else(|| Response::refused(501, "this build keeps no examples and compares no runs"))
}

fn answered(said: Result<Json, Refused>, status: u16) -> Response {
    match said {
        Ok(said) => Response::json(status, &said),
        Err(refused) => refused_by_play(&refused),
    }
}

/// An example's id: 16 lower-case hex digits.
pub fn is_example_id(text: &str) -> bool {
    text.len() == 16 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// A picture's identity: `sha256:` and 64 lower-case hex digits.
pub fn is_identity(text: &str) -> bool {
    text.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    })
}

fn example_of(guarded: &Guarded<'_>) -> Result<String, Response> {
    let id = guarded.params.first().map(String::as_str).unwrap_or("");
    match is_example_id(id) {
        true => Ok(id.to_owned()),
        false => Err(Response::refused(404, "no example has that id")),
    }
}

fn compare(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    let run = |at: usize| {
        let text = guarded.params.get(at).map(String::as_str).unwrap_or("");
        RunId::parse(text).map_err(|_| Response::refused(404, "no run has that id"))
    };
    let (baseline, attempt) = match (run(0), run(1)) {
        (Ok(baseline), Ok(attempt)) => (baseline, attempt),
        (Err(refused), _) | (_, Err(refused)) => return refused,
    };
    match saved(shared) {
        Ok(saved) => answered(saved.compare(&baseline, &attempt), 200),
        Err(refused) => refused,
    }
}

fn examples(shared: &Shared, _: &Guarded<'_>) -> Response {
    match saved(shared) {
        Ok(saved) => answered(saved.examples(), 200),
        Err(refused) => refused,
    }
}

/// A name and notes as a person gave them: a name of 1 to 80 characters that is not all
/// spaces, notes of at most 2,000.
fn checked_words(name: Option<&str>, notes: Option<&str>) -> Result<(), Response> {
    if let Some(name) = name {
        if name.trim().is_empty() {
            return Err(Response::refused(400, "an example has a name"));
        }
        if name.chars().count() > NAME_LIMIT {
            return Err(Response::refused(
                400,
                "an example's name is at most 80 characters",
            ));
        }
    }
    if notes.is_some_and(|notes| notes.chars().count() > NOTES_LIMIT) {
        return Err(Response::refused(
            400,
            "an example's notes are at most 2,000 characters",
        ));
    }
    Ok(())
}

fn field<'a>(body: &'a Json, name: &str) -> Result<Option<&'a str>, Response> {
    match body.get(name) {
        None | Some(Json::Null) => Ok(None),
        Some(Json::String(text)) => Ok(Some(text)),
        Some(_) => Err(Response::refused(400, "a name, notes and a run are texts")),
    }
}

/// The body's fields, when it holds only these.
fn only<'a>(guarded: &'a Guarded<'_>, names: &[&str]) -> Result<&'a Json, Response> {
    let body = guarded.json.as_ref().filter(|body| body.is_object());
    let Some(body) = body else {
        return Err(Response::refused(400, "the body is a JSON object"));
    };
    let fields = body.as_object().map(Map::keys).into_iter().flatten();
    if fields.clone().any(|name| !names.contains(&name.as_str())) {
        return Err(Response::refused(
            400,
            "the body names a field this route does not take, or lacks one",
        ));
    }
    Ok(body)
}

fn save(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    let body = match only(guarded, &["run", "name", "notes"]) {
        Ok(body) => body,
        Err(refused) => return refused,
    };
    let (run, name, notes) = match (
        field(body, "run"),
        field(body, "name"),
        field(body, "notes"),
    ) {
        (Ok(Some(run)), Ok(Some(name)), Ok(notes)) => (run, name, notes.unwrap_or("")),
        (Err(refused), _, _) | (_, Err(refused), _) | (_, _, Err(refused)) => return refused,
        _ => return Response::refused(400, "an example is saved from a run, with a name"),
    };
    if let Err(refused) = checked_words(Some(name), Some(notes)) {
        return refused;
    }
    let Ok(run) = RunId::parse(run) else {
        return Response::refused(404, "no run has that id");
    };
    match saved(shared) {
        Ok(saved) => answered(saved.save_example(&run, name, notes), 201),
        Err(refused) => refused,
    }
}

fn rename(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    let id = match example_of(guarded) {
        Ok(id) => id,
        Err(refused) => return refused,
    };
    let body = match only(guarded, &["name", "notes"]) {
        Ok(body) => body,
        Err(refused) => return refused,
    };
    let (name, notes) = match (field(body, "name"), field(body, "notes")) {
        (Ok(name), Ok(notes)) => (name, notes),
        (Err(refused), _) | (_, Err(refused)) => return refused,
    };
    if name.is_none() && notes.is_none() {
        return Response::refused(400, "a rename gives a name, notes, or both");
    }
    if let Err(refused) = checked_words(name, notes) {
        return refused;
    }
    match saved(shared) {
        Ok(saved) => answered(saved.rename_example(&id, name, notes), 200),
        Err(refused) => refused,
    }
}

fn delete(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    let id = match example_of(guarded) {
        Ok(id) => id,
        Err(refused) => return refused,
    };
    match saved(shared) {
        Ok(saved) => answered(saved.delete_example(&id), 200),
        Err(refused) => refused,
    }
}

/// The draft an example opens: the binary's, and then each backend the example chose
/// prepared with the rest of the draft as given, as the prepare route prepares each
/// choice. A choice this config does not list, or whose backend is not ready now, is
/// taken out of the draft, so the draft takes the config's default, and named in
/// `fell_back` with its kind and why. Nothing is started.
fn open(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    let id = match example_of(guarded) {
        Ok(id) => id,
        Err(refused) => return refused,
    };
    let saved = match saved(shared) {
        Ok(saved) => saved,
        Err(refused) => return refused,
    };
    let mut draft = match saved.open_example(&id) {
        Ok(draft) => draft,
        Err(refused) => return refused_by_play(&refused),
    };
    let action = draft["task"].as_str().unwrap_or_default().to_owned();
    let choices = draft["choices"].as_array().cloned().unwrap_or_default();
    let Some(executor) = shared.executor() else {
        return Response::refused(503, "the playground is stopping");
    };
    let offered = shared.play.choosers(&action);
    let mut fell_back = Vec::new();
    for choice in choices {
        let (Some(argument), Some(value)) = (choice["argument"].as_str(), choice["value"].as_str())
        else {
            continue;
        };
        let rows = offered
            .iter()
            .find(|(name, _)| name == argument)
            .map(|(_, rows)| rows.as_slice())
            .unwrap_or_default();
        let row = rows.iter().find(|row| row["name"] == value);
        let capability = choice["capability"].as_str().unwrap_or_default();
        let said = match row {
            None => Some(json!({
                "argument": argument, "value": escaped(value), "capability": capability,
                "kind": "not_in_config",
                "why": format!("this config names no backend {} now", escaped(value)),
            })),
            Some(row) => {
                let with = draft["values"].as_object().cloned().unwrap_or_default();
                let answer = match shared.play.call(&action, &with) {
                    Ok((call, _)) => {
                        let base = base_refusal(shared, &*executor, &action, &with, argument);
                        let prepared = executor.prepare(&call);
                        choice_json(shared, capability, row, value, prepared, base.as_ref())
                    }
                    Err(_) => own_readiness(shared, capability, row, value),
                };
                (answer["ready"] != true).then(|| {
                    json!({"argument": argument, "value": escaped(value),
                           "capability": capability, "kind": answer["kind"].clone(),
                           "why": answer["why"].clone()})
                })
            }
        };
        if let Some(said) = said {
            if let Some(values) = draft["values"].as_object_mut() {
                values.remove(argument);
            }
            fell_back.push(said);
        }
    }
    drop(executor);
    draft["fell_back"] = Json::Array(fell_back);
    Response::json(200, &draft)
}

fn library(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    let Some(show) = Show::of(guarded.head.field("show")) else {
        return Response::refused(400, "show is all, named, inputs or imported");
    };
    let limit = match guarded.head.field("limit") {
        None => LIBRARY_PAGE_DEFAULT,
        Some(text) => match text.parse::<usize>() {
            Ok(limit) if (1..=LIBRARY_PAGE_LIMIT).contains(&limit) => limit,
            _ => return Response::refused(400, "limit is a number from 1 to 200"),
        },
    };
    let after = guarded.head.field("after");
    if after.is_some_and(|after| !is_identity(after)) {
        return Response::refused(400, "after is a picture's identity");
    }
    match saved(shared) {
        Ok(saved) => answered(saved.library(show, after, limit), 200),
        Err(refused) => refused,
    }
}

fn card(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    let id = guarded.params.first().map(String::as_str).unwrap_or("");
    if !is_identity(id) {
        return Response::refused(404, "the store holds no picture of that identity");
    }
    match saved(shared) {
        Ok(saved) => answered(saved.library_card(id), 200),
        Err(refused) => refused,
    }
}
