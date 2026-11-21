//! Who may call (ADR 0041; the plan's "Who may call"): the checks every request passes,
//! in their order, before a route's handler is given it. [`admit`] is the one function
//! that makes a [`Guarded`], and a route's handler takes nothing else, so no route can be
//! reached past it.
//!
//! Every refusal is in fixed words: it names the check, and nothing the request held.

use std::net::TcpStream;
use std::sync::atomic::Ordering;
use std::time::Instant;

use serde_json::Value as Json;
use whizbang_core::identity::{DecodeLimits, Digest};

use super::Shared;
use super::http::{self, Head, Response, Unread};
use super::routes::{Body, Route, find};

/// The most bytes of a JSON body.
pub const JSON_LIMIT: u64 = 1024 * 1024;
/// The most bytes of a picture.
pub const PICTURE_LIMIT: u64 = 64 * 1024 * 1024;
/// How deep a JSON body nests at most: the stdio server's bound.
pub const DEPTH_LIMIT: usize = crate::frame::DEPTH_LIMIT;

/// A request that passed every check: its head, the parts of its path the route names,
/// and its body, read and checked. Only [`admit`] makes one.
pub struct Guarded<'a> {
    pub head: Head,
    /// The parts of the path the route's pattern names (`{id}`), in order.
    pub params: Vec<String>,
    /// A JSON body, parsed.
    pub json: Option<Json>,
    /// A picture whose header says a size inside the core's limits.
    pub picture: Option<Picture>,
    _upload: Option<Upload<'a>>,
    _sealed: Sealed,
}

/// A picture as the guard took it: its bytes, the digest of those bytes (the file's
/// identity), and the size its header states. It is not decoded here.
pub struct Picture {
    pub bytes: Vec<u8>,
    pub digest: Digest,
    pub width: u32,
    pub height: u32,
}

/// The width and height a PNG's or a baseline or progressive JPEG's header states;
/// none for anything else, or a header cut short.
pub fn dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let be32 = |at: usize| -> Option<u32> {
        Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
    };
    let be16 = |at: usize| -> Option<u32> {
        Some(u32::from(u16::from_be_bytes(
            bytes.get(at..at + 2)?.try_into().ok()?,
        )))
    };
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        if bytes.get(12..16)? != b"IHDR" {
            return None;
        }
        return Some((be32(16)?, be32(20)?));
    }
    if !bytes.starts_with(&[0xff, 0xd8]) {
        return None;
    }
    let mut at = 2;
    loop {
        // Fill bytes before a marker are skipped.
        while *bytes.get(at)? == 0xff && *bytes.get(at + 1)? == 0xff {
            at += 1;
        }
        if *bytes.get(at)? != 0xff {
            return None;
        }
        let marker = *bytes.get(at + 1)?;
        if (0xd0..=0xd9).contains(&marker) || marker == 0x01 {
            at += 2;
            continue;
        }
        let length = usize::try_from(be16(at + 2)?).ok()?;
        let is_frame = matches!(marker, 0xc0..=0xcf) && !matches!(marker, 0xc4 | 0xc8 | 0xcc);
        if is_frame {
            return Some((be16(at + 7)?, be16(at + 5)?));
        }
        if length < 2 {
            return None;
        }
        at += 2 + length;
    }
}

/// What keeps [`Guarded`] from being made outside this module.
struct Sealed;

/// A place among the uploads that hold a worker, given back when the request is done.
struct Upload<'a>(&'a Shared);

impl Drop for Upload<'_> {
    fn drop(&mut self) {
        let mut state = self.0.state();
        state.uploads = state.uploads.saturating_sub(1);
    }
}

fn refused(status: u16, why: &str) -> Unread {
    Unread::Answer(Response::refused(status, why))
}

/// The two names the host answers to, at its port.
fn hosts(port: u16) -> [String; 2] {
    [format!("127.0.0.1:{port}"), format!("localhost:{port}")]
}

/// Reads a request from a connection made at `since` and holds it to every check, in
/// order; the first that fails answers, or closes. What comes back passed them all, with
/// the route it is for.
pub(crate) fn admit<'a>(
    shared: &'a Shared,
    stream: &mut TcpStream,
    since: Instant,
) -> Result<(Guarded<'a>, &'static Route), Unread> {
    let deadlines = &shared.settings.deadlines;
    let (head, start) = http::read_head(stream, since, deadlines)?;

    // The Host: exactly loopback at the port, against a name that a hostile page
    // points at loopback.
    let named = hosts(shared.port);
    if !head
        .header("host")
        .is_some_and(|host| named.iter().any(|n| n == host))
    {
        return Err(refused(
            421,
            "this host answers to 127.0.0.1 and localhost at its port only",
        ));
    }
    if shared.stopping.load(Ordering::SeqCst) {
        return Err(refused(503, "the playground is stopping"));
    }
    let api = head.path == "/api" || head.path.starts_with("/api/");
    if api {
        // The launch token, compared in constant time.
        let presented = head
            .header("authorization")
            .and_then(|value| value.strip_prefix("Bearer "));
        if !presented.is_some_and(|token| shared.token.matches(token.as_bytes())) {
            return Err(refused(
                401,
                "this call needs the token of this launch; run whizbang play again",
            ));
        }
        // The Origin: when sent, the host's own; and sent by anything that changes
        // something.
        let origins = named.map(|host| format!("http://{host}"));
        match head.header("origin") {
            Some(origin) if !origins.iter().any(|o| o == origin) => {
                return Err(refused(403, "a call from another origin is refused"));
            }
            None if head.method != "GET" => {
                return Err(refused(
                    403,
                    "a call that changes something names its origin",
                ));
            }
            _ => {}
        }
    }
    // A request another site makes is refused, but for one thing: the browser going to
    // the page itself, as a document at the top, from wherever it was sent (the
    // opener page `whizbang play` hands Chrome is a file, which Chrome calls another
    // site). That loads the interface's own public files and nothing of the store;
    // the page acts on nothing in its address but the token after `#k=`. Under
    // `/api/` nothing from another site is taken.
    let navigation = !api
        && head.method == "GET"
        && head.header("sec-fetch-mode") == Some("navigate")
        && head.header("sec-fetch-dest") == Some("document");
    if let Some(site) = head.header("sec-fetch-site")
        && site != "same-origin"
        && site != "none"
        && !navigation
    {
        return Err(refused(403, "a request from another site is refused"));
    }

    let (route, params) = find(&head.method, &head.path)?;
    if head
        .query
        .iter()
        .any(|(name, _)| !route.query.contains(&name.as_str()))
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
    match route.body {
        Body::None if head.length > 0 => {
            return Err(refused(413, "this route takes no body"));
        }
        Body::None => {}
        Body::Json => {
            if head.length > JSON_LIMIT {
                return Err(refused(413, "a JSON body is at most 1 MiB"));
            }
            if declared.as_deref() != Some("application/json") {
                return Err(refused(415, "this route takes application/json"));
            }
        }
        Body::Picture => {
            if head.length > PICTURE_LIMIT {
                return Err(refused(413, "a picture is at most 64 MiB"));
            }
            if !matches!(declared.as_deref(), Some("image/png" | "image/jpeg")) {
                return Err(refused(415, "this route takes image/png or image/jpeg"));
            }
        }
    }
    // An upload holds a worker for its time: at most two at once, refused as soon as
    // the headers are read.
    let upload = match route.body {
        Body::Picture => {
            let mut state = shared.state();
            if state.uploads >= super::UPLOADS_LIMIT {
                return Err(Unread::Answer(
                    Response::refused(429, "at most two pictures are sent at once")
                        .with("Retry-After", "1"),
                ));
            }
            state.uploads += 1;
            Some(Upload(shared))
        }
        _ => None,
    };
    let body = match route.body {
        Body::None => Vec::new(),
        _ => http::read_body(stream, head.length, start, deadlines)?,
    };
    let mut json = None;
    let mut picture = None;
    match route.body {
        Body::None => {}
        Body::Json => {
            if crate::frame::depth(&body) > DEPTH_LIMIT {
                return Err(refused(400, "a JSON body nests at most 16 deep"));
            }
            match serde_json::from_slice::<Json>(&body) {
                Ok(parsed) => json = Some(parsed),
                Err(_) => return Err(refused(400, "the body is not JSON")),
            }
        }
        Body::Picture => {
            // Its size, from its header alone: nothing is decoded here. The handler
            // that keeps a picture decodes it within the core's limits as it keeps it.
            let Some((width, height)) = dimensions(&body) else {
                return Err(refused(415, "the body is not a PNG or JPEG"));
            };
            let pixels = u64::from(width) * u64::from(height);
            let most = DecodeLimits::default().max_pixels;
            if pixels > most {
                return Err(refused(
                    413,
                    &format!(
                        "the picture is {width}x{height}, {pixels} pixels, over the limit of {most}"
                    ),
                ));
            }
            picture = Some(Picture {
                digest: Digest::of_bytes(&body),
                width,
                height,
                bytes: body,
            });
        }
    }
    Ok((
        Guarded {
            head,
            params,
            json,
            picture,
            _upload: upload,
            _sealed: Sealed,
        },
        route,
    ))
}

#[cfg(test)]
mod tests {
    use super::dimensions;

    #[test]
    fn a_pictures_size_is_read_from_its_header_alone() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        png.extend_from_slice(&640u32.to_be_bytes());
        png.extend_from_slice(&480u32.to_be_bytes());
        assert_eq!(dimensions(&png), Some((640, 480)));
        // A JPEG: an application segment, fill bytes, then its frame.
        let jpeg = [
            0xff, 0xd8, 0xff, 0xe0, 0x00, 0x04, 0x00, 0x00, 0xff, 0xff, 0xc2, 0x00, 0x11, 0x08,
            0x01, 0xe0, 0x02, 0x80,
        ];
        assert_eq!(dimensions(&jpeg), Some((640, 480)));
        for not in [
            &b"hello"[..],
            &png[..20],
            &jpeg[..12],
            &[0xff, 0xd8, 0x00][..],
        ] {
            assert_eq!(dimensions(not), None, "{not:?}");
        }
    }
}
