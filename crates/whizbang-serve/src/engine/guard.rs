//! Who may call the engine API (ADR 0045, "Who may call"): the checks every request
//! passes, in their order, on a reader, before a handler is given it. [`serve`] is the one
//! function that makes a [`Guarded`], and a handler takes nothing else, so no route can be
//! reached past it.
//!
//! The reader reads a request's line and headers within [`super::Times::head`] of its
//! acceptance, and, for every route but an upload, its whole body at no less than
//! [`super::Times::rate`] after that; so it knows the request's route, its method, its tool
//! and its request id before any handler is taken. A repeat of a run that is going here
//! takes over that run's stream there and then, with no handler at all.
//!
//! Every refusal is in fixed words: it names the check, and nothing the request held.

use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{Map, Value as Json};
use whizbang_core::contract::{Call, RequestId, RunId};
use whizbang_core::identity::Digest;

use super::mcp::{rpc_error, tool_answer};
use super::{
    Admitting, Follower, Held, Lookup, PROTOCOL_VERSION, Settings, Shared, VERSION_HEADER,
};
use crate::escaped;
use crate::http::{self, Deadlines, Head, Response, Unread};

/// The most bytes of a JSON body: a message of the stdio server's bound.
pub const JSON_LIMIT: u64 = crate::frame::MESSAGE_LIMIT as u64;
/// The most bytes of a picture.
pub const PICTURE_LIMIT: u64 = 64 * 1024 * 1024;
/// How deep a JSON body nests at most: the stdio server's bound.
pub const DEPTH_LIMIT: usize = crate::frame::DEPTH_LIMIT;
/// The most characters of a request's id, or of a progress token, the server takes.
pub const ID_LIMIT: usize = 128;

/// The engine's token: at least [`Secret::LEAST`] characters of printable ASCII, given at
/// start. Its `Debug` and `Display` print nothing, and it is never written anywhere.
#[derive(Clone)]
pub struct Secret(Vec<u8>);

impl Secret {
    /// The fewest characters of a token.
    pub const LEAST: usize = 32;

    /// The token this text is: refused, in words that hold none of it, when it is shorter
    /// than [`Secret::LEAST`] or holds anything but printable ASCII without spaces.
    pub fn new(text: &str) -> Result<Secret, String> {
        let text = text.strip_suffix('\n').unwrap_or(text);
        let text = text.strip_suffix('\r').unwrap_or(text);
        if !text.bytes().all(|b| b.is_ascii_graphic()) {
            return Err("the token holds only printable ASCII, with no spaces".to_owned());
        }
        if text.len() < Secret::LEAST {
            return Err(format!(
                "the token is at least {} characters",
                Secret::LEAST
            ));
        }
        Ok(Secret(text.as_bytes().to_vec()))
    }

    /// Whether `presented` is the token, compared in constant time for tokens of its length.
    pub fn matches(&self, presented: &[u8]) -> bool {
        let mut differ = u8::from(presented.len() != self.0.len());
        for (at, own) in self.0.iter().enumerate() {
            differ |= own ^ presented.get(at).copied().unwrap_or(0);
        }
        differ == 0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret")
    }
}

impl std::fmt::Display for Secret {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}

/// The `Host` values a request may name: loopback's three names at the port, when the
/// host listens on loopback; and each host name, as given and at the port.
pub(crate) fn names(settings: &Settings, address: SocketAddr) -> Vec<String> {
    let port = address.port();
    let mut names = Vec::new();
    if address.ip().is_loopback() {
        for name in ["127.0.0.1", "localhost", "[::1]"] {
            names.push(format!("{name}:{port}"));
        }
    }
    for name in &settings.host_names {
        names.push(name.clone());
        // A name with no port of its own is answered at the server's port too.
        let has_port = match name.strip_prefix('[') {
            Some(bracketed) => !bracketed.ends_with(']'),
            None => name.contains(':'),
        };
        if !has_port {
            names.push(format!("{name}:{port}"));
        }
    }
    names
}

/// Whether a text reads as a path: `/`, `\` or `~` first, a drive letter, `file:`, or a
/// `..` part. A picture is named by the identity its upload was answered with, never by
/// where a file is.
pub fn reads_as_path(text: &str) -> bool {
    let text = text.trim_start();
    let bytes = text.as_bytes();
    let drive = bytes.len() >= 2
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes.len() == 2 || bytes[2] == b'/' || bytes[2] == b'\\');
    text.starts_with(['/', '\\', '~'])
        || drive
        || text
            .get(..5)
            .is_some_and(|start| start.eq_ignore_ascii_case("file:"))
        || text.split(['/', '\\']).any(|part| part == "..")
}

/// Whether any string in this value, at any depth, reads as a path.
pub fn holds_a_path(value: &Json) -> bool {
    match value {
        Json::String(text) => reads_as_path(text),
        Json::Array(items) => items.iter().any(holds_a_path),
        Json::Object(fields) => fields.values().any(holds_a_path),
        _ => false,
    }
}

/// Which handlers serve a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Share {
    Stream,
    Upload,
    Run,
    Short,
}

/// What a request asks, as the reader read it.
pub(crate) enum Asked {
    Initialize {
        id: Json,
    },
    Ping {
        id: Json,
    },
    List {
        id: Json,
    },
    Unknown {
        id: Json,
    },
    /// A call that makes a run: its place is taken.
    Call {
        id: Json,
        tool: String,
        token: Option<Json>,
        call: Call,
        request: Option<RequestId>,
        place: Admitting,
    },
    /// A call whose request id names a run already made, which no handler follows here.
    Repeat {
        id: Json,
        tool: String,
        token: Option<Json>,
        run: RunId,
    },
    Upload {
        mask_of: Option<String>,
        _place: UploadPlace,
    },
    Artifact(String),
    Status(String),
    Cancel(String),
}

/// A place among the uploads being read, given back when dropped.
pub(crate) struct UploadPlace(Arc<Shared>);

impl Drop for UploadPlace {
    fn drop(&mut self) {
        self.0.give_upload();
    }
}

/// What keeps [`Guarded`] from being made outside this module.
struct Sealed;

/// A request that passed every check, read and parsed: only [`serve`] makes one.
pub struct Guarded {
    pub(crate) conn: TcpStream,
    pub(crate) since: Instant,
    pub(crate) head: Head,
    /// What came after the headers: the start of an upload's body.
    pub(crate) rest: Vec<u8>,
    pub(crate) asked: Asked,
    pub(crate) held: Held,
    _sealed: Sealed,
}

impl Guarded {
    pub(crate) fn share(&self) -> Share {
        match self.asked {
            Asked::Call { .. } => Share::Stream,
            Asked::Upload { .. } => Share::Upload,
            Asked::Status(_) | Asked::Cancel(_) => Share::Run,
            _ => Share::Short,
        }
    }

    /// Serves it, on a handler of its share.
    pub(crate) fn serve(self, shared: &Arc<Shared>) {
        match self.share() {
            Share::Stream => super::mcp::stream(shared, self),
            Share::Upload => super::pictures::upload(shared, self),
            Share::Run => super::runs::serve(shared, self),
            Share::Short => match self.asked {
                Asked::Artifact(_) => super::pictures::download(shared, self),
                _ => super::mcp::short(shared, self),
            },
        }
    }
}

/// The deadlines of the shared HTTP code, from the engine's times.
pub(crate) fn deadlines(shared: &Shared) -> Deadlines {
    let times = shared.settings.times;
    Deadlines {
        head: times.head,
        body_grace: times.head,
        rate: times.rate,
        answer: times.answer,
    }
}

fn refused(status: u16, why: &str) -> Unread {
    Unread::Answer(Response::refused(status, why))
}

fn rpc_refused(status: u16, id: &Json, code: i64, why: &str) -> Unread {
    Unread::Answer(Response::json(status, &rpc_error(id, code, why)))
}

/// Reads a connection accepted at `since` and holds it to every check, in order: the
/// first that fails answers, or closes. What passed them all goes to a handler of its
/// share; a repeat of a run going here is handed to that run's handler.
pub(crate) fn serve(shared: &Arc<Shared>, mut conn: TcpStream, since: Instant, held: Held) {
    let deadlines = deadlines(shared);
    let (head, rest, asked) = match admit(shared, &mut conn, since) {
        Ok(read) => read,
        Err(Unread::Answer(response)) => return response.send(&mut conn, &deadlines),
        Err(Unread::Close) => return,
    };
    shared.read(held.number);
    // A repeat of a run that a handler follows here takes over its stream, at once.
    let asked = match asked {
        Asked::Repeat {
            id,
            tool,
            token,
            run,
        } => {
            let follower = Follower {
                conn,
                id,
                token,
                held: Some(held),
            };
            match shared.attach_waiting(&run, follower) {
                Ok(()) => return,
                Err(back) => {
                    let Follower {
                        conn: back_conn,
                        id,
                        token,
                        held: back_held,
                    } = back;
                    let Some(back_held) = back_held else {
                        return;
                    };
                    let guarded = Guarded {
                        conn: back_conn,
                        since,
                        head,
                        rest,
                        asked: Asked::Repeat {
                            id,
                            tool,
                            token,
                            run,
                        },
                        held: back_held,
                        _sealed: Sealed,
                    };
                    return hand(shared, Box::new(guarded));
                }
            }
        }
        other => other,
    };
    let guarded = Guarded {
        conn,
        since,
        head,
        rest,
        asked,
        held,
        _sealed: Sealed,
    };
    hand(shared, Box::new(guarded));
}

/// Hands a request to its share; a share whose queue is full answers 429.
fn hand(shared: &Arc<Shared>, guarded: Box<Guarded>) {
    if let Err(mut back) = shared.hand(guarded) {
        let answer = match shared.stopping.load(std::sync::atomic::Ordering::SeqCst) {
            true => Response::refused(503, "the engine is stopping"),
            false => Response::refused(429, "every handler of this kind of request is busy")
                .with("Retry-After", "1"),
        };
        answer.send(&mut back.conn, &deadlines(shared));
    }
}

/// The route of a request, as its method and path name it.
enum Route {
    Mcp,
    Upload,
    Artifact(String),
    Status(String),
    Cancel(String),
}

fn route(method: &str, path: &str) -> Result<Route, Unread> {
    let parts: Vec<&str> = path.split('/').collect();
    let (route, wants) = match parts.as_slice() {
        ["", "mcp"] => (Route::Mcp, "POST"),
        ["", "v1", "uploads"] => (Route::Upload, "POST"),
        ["", "v1", "artifacts", id] if !id.is_empty() => (Route::Artifact((*id).to_owned()), "GET"),
        ["", "v1", "runs", id] if !id.is_empty() => (Route::Status((*id).to_owned()), "GET"),
        ["", "v1", "runs", id, "cancel"] if !id.is_empty() => {
            (Route::Cancel((*id).to_owned()), "POST")
        }
        _ => return Err(refused(404, "no such route")),
    };
    if method != wants {
        return Err(refused(405, "this route takes another method"));
    }
    Ok(route)
}

/// The checks, in their order, and the request read and parsed.
fn admit(
    shared: &Arc<Shared>,
    conn: &mut TcpStream,
    since: Instant,
) -> Result<(Head, Vec<u8>, Asked), Unread> {
    let deadlines = deadlines(shared);
    let (head, rest) = http::read_head(conn, since, &deadlines)?;

    // The Host: one of the server's names at its port.
    if !head
        .header("host")
        .is_some_and(|host| shared.names.iter().any(|name| name == host))
    {
        return Err(refused(421, "this server answers to its own names only"));
    }
    if shared.stopping.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(refused(503, "the engine is stopping"));
    }
    // The token, compared in constant time. The refusal names nothing.
    let presented = head
        .header("authorization")
        .and_then(|value| value.strip_prefix("Bearer "));
    if !presented.is_some_and(|token| shared.token.matches(token.as_bytes())) {
        return Err(refused(401, "this call needs the engine's token"));
    }
    // The Origin: when sent, one listed. None is listed by default.
    if let Some(origin) = head.header("origin")
        && !shared.settings.allow_origins.iter().any(|o| o == origin)
    {
        return Err(refused(403, "a call from this origin is refused"));
    }

    let route = route(&head.method, &head.path)?;
    let takes: &[&str] = match route {
        Route::Upload => &["mask_of"],
        _ => &[],
    };
    if head
        .query
        .iter()
        .any(|(name, _)| !takes.contains(&name.as_str()))
    {
        return Err(refused(
            400,
            "the address names a field this route does not take",
        ));
    }
    let declared = head.header("content-type").map(|value| {
        value
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
    });
    match route {
        Route::Mcp => {
            if head.length > JSON_LIMIT {
                return Err(refused(413, "a JSON body is at most 1 MiB"));
            }
            if declared.as_deref() != Some("application/json") {
                return Err(refused(415, "this route takes application/json"));
            }
            let until = since
                + shared.settings.times.head
                + rate_time(head.length, shared.settings.times.rate);
            let body = http::read_body_until(conn, head.length, rest, until)?;
            let asked = message(shared, &head, &body)?;
            Ok((head, Vec::new(), asked))
        }
        Route::Upload => {
            if head.length > PICTURE_LIMIT {
                return Err(refused(413, "a picture is at most 64 MiB"));
            }
            let mask_of = head.field("mask_of").map(str::to_owned);
            match (&mask_of, declared.as_deref()) {
                (None, Some("image/png" | "image/jpeg")) | (Some(_), Some("image/png")) => {}
                (Some(_), _) => return Err(refused(415, "a mask is sent as a grey image/png")),
                (None, _) => return Err(refused(415, "this route takes image/png or image/jpeg")),
            }
            if let Some(of) = &mask_of
                && of.parse::<Digest>().is_err()
            {
                return Err(refused(400, "mask_of is an image's identity"));
            }
            if head.length == 0 {
                return Err(refused(400, "an upload holds a picture"));
            }
            if !shared.take_upload() {
                return Err(Unread::Answer(
                    Response::refused(429, "at most two pictures are sent at once")
                        .with("Retry-After", "1"),
                ));
            }
            let place = UploadPlace(shared.clone());
            Ok((
                head,
                rest,
                Asked::Upload {
                    mask_of,
                    _place: place,
                },
            ))
        }
        Route::Artifact(id) | Route::Status(id) | Route::Cancel(id) if head.length > 0 => {
            let _ = id;
            Err(refused(413, "this route takes no body"))
        }
        Route::Artifact(id) => Ok((head, rest, Asked::Artifact(id))),
        Route::Status(id) => Ok((head, rest, Asked::Status(id))),
        Route::Cancel(id) => Ok((head, rest, Asked::Cancel(id))),
    }
}

/// How long `length` bytes take at `rate` bytes a second.
fn rate_time(length: u64, rate: u64) -> Duration {
    Duration::from_secs_f64(length as f64 / rate.max(1) as f64)
}

/// A JSON-RPC message's id as the server takes one.
fn taken_id(id: &Json) -> Option<Json> {
    match id {
        Json::String(text) if text.chars().count() <= ID_LIMIT => Some(id.clone()),
        Json::Number(number) if number.is_i64() => Some(id.clone()),
        _ => None,
    }
}

/// The message a body of `POST /mcp` holds, read as far as a handler needs it.
fn message(shared: &Arc<Shared>, head: &Head, body: &[u8]) -> Result<Asked, Unread> {
    if crate::frame::depth(body) > DEPTH_LIMIT {
        return Err(rpc_refused(
            400,
            &Json::Null,
            -32600,
            "a message nests at most 16 deep",
        ));
    }
    let Ok(message) = serde_json::from_slice::<Json>(body) else {
        return Err(rpc_refused(
            400,
            &Json::Null,
            -32700,
            "the body is not JSON",
        ));
    };
    let Some(message) = message.as_object() else {
        return Err(rpc_refused(
            400,
            &Json::Null,
            -32600,
            "a body is one message: a batch is not taken",
        ));
    };
    if message.get("jsonrpc") != Some(&Json::from("2.0")) {
        return Err(rpc_refused(
            400,
            &Json::Null,
            -32600,
            "a message says jsonrpc 2.0",
        ));
    }
    let method = message.get("method").and_then(Json::as_str);
    if method != Some("initialize") && head.header(VERSION_HEADER) != Some(PROTOCOL_VERSION) {
        return Err(rpc_refused(
            400,
            &Json::Null,
            -32600,
            "every message but initialize names MCP-Protocol-Version: 2025-06-18",
        ));
    }
    // A client's answer, or a notification: taken, and nothing is answered.
    let (Some(method), Some(id)) = (method, message.get("id")) else {
        return Err(Unread::Answer(Response::bytes(
            202,
            "application/json",
            Vec::new(),
        )));
    };
    let Some(id) = taken_id(id) else {
        let why = format!("an id is a whole number or a string of at most {ID_LIMIT}");
        return Err(rpc_refused(400, &Json::Null, -32600, &why));
    };
    let params = match message.get("params") {
        None => Map::new(),
        Some(Json::Object(params)) => params.clone(),
        Some(_) => return Err(rpc_refused(400, &id, -32602, "params are an object")),
    };
    match method {
        // Answered with the one revision offered, whatever the client asked.
        "initialize" => Ok(Asked::Initialize { id }),
        "ping" => Ok(Asked::Ping { id }),
        "tools/list" => Ok(Asked::List { id }),
        "tools/call" => call(shared, id, &params),
        _ => Ok(Asked::Unknown { id }),
    }
}

/// A `tools/call`: its tool and arguments checked, its request id looked up, and a place
/// taken for a new run.
fn call(shared: &Arc<Shared>, id: Json, params: &Map<String, Json>) -> Result<Asked, Unread> {
    let answered = |answer: Json| Unread::Answer(Response::json(200, &answer));
    let Some(tool) = params.get("name").and_then(Json::as_str) else {
        return Err(answered(rpc_error(&id, -32602, "a call names its tool")));
    };
    let arguments = match params.get("arguments") {
        None => Map::new(),
        Some(Json::Object(arguments)) => arguments.clone(),
        Some(_) => {
            return Err(answered(rpc_error(
                &id,
                -32602,
                "a call's arguments are an object",
            )));
        }
    };
    let door = shared.hosted.as_ref();
    if !door.tools().iter().any(|offered| offered.name == tool) {
        return Err(answered(rpc_error(
            &id,
            -32602,
            "the server offers no tool of that name",
        )));
    }
    if arguments.values().any(holds_a_path) {
        return Err(rpc_refused(
            400,
            &id,
            -32602,
            "an argument reads as a path: a picture is named by the identity its upload was answered with",
        ));
    }
    let token = params
        .get("_meta")
        .and_then(|meta| meta.get("progressToken"))
        .filter(|token| match token {
            Json::String(text) => text.chars().count() <= ID_LIMIT,
            Json::Number(number) => number.is_i64(),
            _ => false,
        })
        .cloned();
    let tool = tool.to_owned();
    let (call, request_id) = match door.call(&tool, &arguments) {
        Ok(asked) => asked,
        Err(why) => return Err(answered(tool_answer(&id, &why, true, None))),
    };
    let request = match request_id {
        None => None,
        Some(text) => {
            let Some(executor) = shared.executor() else {
                return Err(refused(503, "the engine is stopping"));
            };
            let request = match executor.request_id(&text) {
                Ok(request) => request,
                Err(refusal) => {
                    return Err(answered(tool_answer(&id, &refusal.to_string(), true, None)));
                }
            };
            let looked = super::look_up(&*executor, &request, &call);
            drop(executor);
            let looked = match looked {
                Ok(looked) => looked,
                Err(refusal) => {
                    return Err(answered(tool_answer(&id, &refusal.to_string(), true, None)));
                }
            };
            match looked {
                Lookup::Missing => Some(request),
                Lookup::Same(run) => {
                    return Ok(Asked::Repeat {
                        id,
                        tool,
                        token,
                        run,
                    });
                }
                Lookup::Other(run) => {
                    let text = format!(
                        "conflict: another request was sent under this request id, as run {}; nothing ran",
                        escaped(run.as_str())
                    );
                    return Err(answered(tool_answer(&id, &text, true, Some(&run))));
                }
            }
        }
    };
    let Some(place) = shared.admit() else {
        if shared.stopping.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(refused(503, "the engine is stopping"));
        }
        let why = format!(
            "at most {} runs started here go at once; try again when one ends",
            shared.settings.calls
        );
        return Err(Unread::Answer(
            Response::json(429, &rpc_error(&id, -32000, &why)).with("Retry-After", "5"),
        ));
    };
    Ok(Asked::Call {
        id,
        tool,
        token,
        call,
        request,
        place,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_is_read_in_each_of_its_forms_and_an_identity_or_words_are_not() {
        for path in [
            "/etc/passwd",
            "~/x.png",
            "\\\\server\\share",
            "C:\\x.png",
            "c:/x.png",
            "C:",
            "file:///x.png",
            "FILE:x",
            "pictures/../../x",
            "..",
            "a\\..\\b",
            "  /etc",
        ] {
            assert!(reads_as_path(path), "{path}");
        }
        for not in [
            "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "the cat on the left",
            "a/b",
            "wait...",
            "files: none",
            "",
        ] {
            assert!(!reads_as_path(not), "{not}");
        }
        assert!(holds_a_path(&serde_json::json!({"a": [{"b": "/x"}]})));
        assert!(!holds_a_path(&serde_json::json!({"a": [1, true, "cat"]})));
    }

    #[test]
    fn a_token_is_long_enough_and_printable_and_prints_nothing() {
        assert!(Secret::new("short").is_err());
        assert!(Secret::new(&"a".repeat(31)).is_err());
        assert!(Secret::new(&format!("{} b", "a".repeat(32))).is_err());
        assert!(Secret::new(&format!("{}\u{e9}", "a".repeat(32))).is_err());
        let token = "t0k3n-".repeat(6);
        let secret = Secret::new(&format!("{token}\n")).unwrap();
        assert!(secret.matches(token.as_bytes()));
        assert!(!secret.matches(&token.as_bytes()[1..]));
        assert!(!secret.matches(format!("{token}x").as_bytes()));
        assert_eq!(format!("{secret}"), "");
        assert_eq!(format!("{secret:?}"), "Secret");
    }
}
