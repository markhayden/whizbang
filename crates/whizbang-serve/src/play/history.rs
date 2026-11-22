//! History, its figures, a run's report and its export, and the tool records (plan m5,
//! task W3; ADR 0041, "Routes"): the routes, registered by one line in
//! [`super::routes`], and what the binary answers them with, [`Shelf`].
//!
//! - `GET /api/runs?…`: the runs of the store, newest first, by pages, under the filter
//!   of `runs stats` and the page's kit, task, result and words. `open=1` is still W1's
//!   list of the runs going ([`super::routes`]).
//! - `GET /api/stats?…`: the figures of `runs stats` over the same filter.
//! - `GET /api/runs/{id}/report`: the run's report, one self-contained HTML document
//!   with a policy of its own and no script, for the page's isolated frame and for
//!   "Export".
//! - `GET /api/runs/{id}/export`: the run's record and outputs, one archive.
//! - `GET /api/records`, `/api/records/{name}` and `/api/records/{name}/{file}`: the
//!   tool records the binary embeds ([`super::records`]).

use serde_json::{Map, Value as Json, json};
use whizbang_core::contract::RunId;

use super::guard::Guarded;
use super::http::Response;
use super::routes::{Body, Route};
use super::{Refused, Shared, records};
use crate::escaped;

/// The fields of the filter, as the address names them: `runs stats`'s, then the page's.
pub const FILTER: &[&str] = &[
    "action", "model", "backend", "use_case", "matrix", "since", "until", "kit", "task", "result",
    "words",
];

/// The fields `GET /api/runs` takes: the filter, a page's place and size, and W1's
/// `open`.
pub const LISTED: &[&str] = &[
    "action", "model", "backend", "use_case", "matrix", "since", "until", "kit", "task", "result",
    "words", "before", "limit", "open",
];

/// The most runs one page of history lists, and how many when the page does not say.
pub const PAGE_LIMIT: usize = 200;
pub const PAGE_DEFAULT: usize = 50;

/// The most bytes a field of the filter holds.
pub const FIELD_LIMIT: usize = 256;

/// What the page asked history or the figures for: each field of the filter it gave,
/// as given, and for history the run a page starts after and how many it lists.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Asked {
    pub filter: Map<String, Json>,
    pub before: Option<String>,
    pub limit: usize,
}

/// A document the binary made for a download: its file's name, its type and its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    /// The file's name, of letters, digits, `-`, `_` and `.` only.
    pub name: String,
    pub content_type: &'static str,
    pub bytes: Vec<u8>,
}

/// What the binary answers history, the figures, a report and an export with. It holds
/// no HTTP type.
pub trait Shelf: Send + Sync {
    /// One page of history: `{"runs": [...], "more": bool}`.
    fn runs(&self, asked: &Asked) -> Result<Json, Refused>;
    /// The figures of `runs stats` over the filter.
    fn stats(&self, asked: &Asked) -> Result<Json, Refused>;
    /// The run's report, an HTML document.
    fn report(&self, run: &RunId) -> Result<Document, Refused>;
    /// The run's record and outputs, an archive.
    fn export(&self, run: &RunId) -> Result<Document, Refused>;
}

/// The routes of this module, which the route table lists after its own.
pub(crate) const ROUTES: &[Route] = &[
    Route {
        method: "GET",
        pattern: "/api/stats",
        body: Body::None,
        query: FILTER,
        handler: stats,
    },
    Route {
        method: "GET",
        pattern: "/api/runs/{id}/report",
        body: Body::None,
        query: &[],
        handler: report,
    },
    Route {
        method: "GET",
        pattern: "/api/runs/{id}/export",
        body: Body::None,
        query: &[],
        handler: export,
    },
    Route {
        method: "GET",
        pattern: "/api/records",
        body: Body::None,
        query: &[],
        handler: record_names,
    },
    Route {
        method: "GET",
        pattern: "/api/records/{id}",
        body: Body::None,
        query: &[],
        handler: record,
    },
    Route {
        method: "GET",
        pattern: "/api/records/{id}/{id}",
        body: Body::None,
        query: &[],
        handler: record_file,
    },
];

/// The policy a report carries when it is served: what its own `<meta>` says, so that a
/// report opened alone loads nothing and runs nothing.
pub const REPORT_POLICY: &str = "default-src 'none'; img-src data:; style-src 'unsafe-inline'; base-uri 'none'; \
     form-action 'none'; sandbox";

fn refused(refused: &Refused) -> Response {
    Response::refused(refused.status, &escaped(&refused.why))
}

/// The filter, a page's place and its size from the address; or a refusal.
fn asked(guarded: &Guarded<'_>) -> Result<Asked, Response> {
    let mut filter = Map::new();
    for (name, value) in &guarded.head.query {
        if value.len() > FIELD_LIMIT {
            return Err(Response::refused(
                400,
                "a field of the filter is at most 256 bytes",
            ));
        }
        if FILTER.contains(&name.as_str()) && !value.is_empty() {
            filter.insert(name.clone(), json!(value));
        }
    }
    let limit = match guarded.head.field("limit") {
        None => PAGE_DEFAULT,
        Some(text) => match text.parse::<usize>() {
            Ok(n) if (1..=PAGE_LIMIT).contains(&n) => n,
            _ => return Err(Response::refused(400, "limit is a number from 1 to 200")),
        },
    };
    let before = guarded.head.field("before").map(str::to_owned);
    if let Some(before) = &before
        && RunId::parse(before).is_err()
    {
        return Err(Response::refused(400, "before names no run"));
    }
    Ok(Asked {
        filter,
        before,
        limit,
    })
}

fn shelf(shared: &Shared) -> Result<&dyn Shelf, Response> {
    shared
        .play
        .shelf()
        .ok_or_else(|| Response::refused(501, "this build keeps no history"))
}

/// `GET /api/runs`: history, or, with `open=1`, the runs still going (W1's).
pub(crate) fn runs(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    if guarded.head.field("open").is_some() {
        return super::routes::open(shared, guarded);
    }
    let asked = match asked(guarded) {
        Ok(asked) => asked,
        Err(refused) => return refused,
    };
    match shelf(shared) {
        Ok(shelf) => match shelf.runs(&asked) {
            Ok(listed) => Response::json(200, &listed),
            Err(why) => refused(&why),
        },
        Err(refused) => refused,
    }
}

fn stats(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    let asked = match asked(guarded) {
        Ok(asked) => asked,
        Err(refused) => return refused,
    };
    match shelf(shared) {
        Ok(shelf) => match shelf.stats(&asked) {
            Ok(figures) => Response::json(200, &figures),
            Err(why) => refused(&why),
        },
        Err(refused) => refused,
    }
}

fn run_of(guarded: &Guarded<'_>) -> Result<RunId, Response> {
    let text = guarded.params.first().map(String::as_str).unwrap_or("");
    RunId::parse(text).map_err(|_| Response::refused(404, "no run has that id"))
}

/// A document as a download: its type, its name, and nothing that would let it run.
fn download(document: Document) -> Response {
    let name: String = (document.name.chars())
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect();
    let disposition = format!("attachment; filename=\"{name}\"");
    Response::bytes(200, document.content_type, document.bytes)
        .with("Content-Disposition", &disposition)
        .with("Content-Security-Policy", REPORT_POLICY)
}

fn report(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    let run = match run_of(guarded) {
        Ok(run) => run,
        Err(refused) => return refused,
    };
    match shelf(shared).map(|shelf| shelf.report(&run)) {
        Ok(Ok(document)) => download(document),
        Ok(Err(why)) => refused(&why),
        Err(refused) => refused,
    }
}

fn export(shared: &Shared, guarded: &Guarded<'_>) -> Response {
    let run = match run_of(guarded) {
        Ok(run) => run,
        Err(refused) => return refused,
    };
    match shelf(shared).map(|shelf| shelf.export(&run)) {
        Ok(Ok(document)) => download(document),
        Ok(Err(why)) => refused(&why),
        Err(refused) => refused,
    }
}

fn record_names(_: &Shared, _: &Guarded<'_>) -> Response {
    Response::json(
        200,
        &json!({"records": records::names().collect::<Vec<_>>()}),
    )
}

fn record(_: &Shared, guarded: &Guarded<'_>) -> Response {
    let name = guarded.params.first().map(String::as_str).unwrap_or("");
    match records::find(&format!("{name}.md")) {
        Some((content_type, bytes)) => Response::bytes(200, content_type, bytes.to_vec()),
        None => Response::refused(404, "no tool or task has a record of that name"),
    }
}

fn record_file(_: &Shared, guarded: &Guarded<'_>) -> Response {
    let (Some(name), Some(file)) = (guarded.params.first(), guarded.params.get(1)) else {
        return Response::refused(404, "no such file");
    };
    match records::find(&format!("{name}/{file}")) {
        Some((content_type, bytes)) => Response::bytes(200, content_type, bytes.to_vec()),
        None => Response::refused(404, "no such file"),
    }
}
