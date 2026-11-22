//! HTTP/1.1 as the host speaks it (ADR 0041): one request on a connection, its line and
//! headers held to [`HEAD_LIMIT`] bytes and read within a deadline, a body sent with
//! `Content-Length` alone and read at no less than a rate, and one answer taken within a
//! deadline, after which the connection is closed. Nothing here decides who may call:
//! that is [`super::guard`].

use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::time::{Duration, Instant};

use serde_json::Value as Json;

/// The most bytes of a request's line and headers together.
pub const HEAD_LIMIT: usize = 16 * 1024;

/// How long each part of a connection may take (ADR 0041, "Who may call").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deadlines {
    /// From the connection to the end of the request's headers.
    pub head: Duration,
    /// How long a body may take before it is held to [`Deadlines::rate`].
    pub body_grace: Duration,
    /// The fewest bytes a second a body must arrive at, on average, after its grace.
    pub rate: u64,
    /// How long the client may take to take the answer.
    pub answer: Duration,
}

impl Default for Deadlines {
    fn default() -> Deadlines {
        Deadlines {
            head: Duration::from_secs(5),
            body_grace: Duration::from_secs(5),
            rate: 1024 * 1024,
            answer: Duration::from_secs(10),
        }
    }
}

/// A request's line and headers, as read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    pub method: String,
    /// The path, before any `?`, as sent: nothing in it is decoded.
    pub path: String,
    /// The query's fields, each decoded, in the order sent.
    pub query: Vec<(String, String)>,
    /// Each header, its name in lower case, its value without the white space around it.
    pub headers: Vec<(String, String)>,
    /// What `Content-Length` says, or none.
    pub length: u64,
}

impl Head {
    /// The value of the header of this name (in lower case), when it was sent.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(named, _)| named == name)
            .map(|(_, value)| value.as_str())
    }

    /// The value of the query's field of this name, when it was sent.
    pub fn field(&self, name: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(named, _)| named == name)
            .map(|(_, value)| value.as_str())
    }
}

/// What reading a part of a request came to when it did not come to the part.
#[derive(Debug)]
pub enum Unread {
    /// Answer this, and close.
    Answer(Response),
    /// Close, and answer nothing: the client went, or was too slow.
    Close,
}

/// The headers that only one of may be sent: two would leave which one counts to whoever
/// reads them.
const SINGLE: [&str; 7] = [
    "host",
    "authorization",
    "content-length",
    "content-type",
    "origin",
    "sec-fetch-site",
    "transfer-encoding",
];

/// Reads a request's line and headers from a connection made at `since`: what came after
/// them is the start of the body.
pub fn read_head(
    stream: &mut TcpStream,
    since: Instant,
    deadlines: &Deadlines,
) -> Result<(Head, Vec<u8>), Unread> {
    let mut buffer: Vec<u8> = Vec::with_capacity(2048);
    let mut piece = [0u8; 4096];
    let end = loop {
        if let Some(at) = find(&buffer, b"\r\n\r\n") {
            break at;
        }
        if buffer.len() > HEAD_LIMIT {
            return Err(Unread::Answer(Response::refused(
                431,
                "a request's line and headers are at most 16 KiB",
            )));
        }
        let left = deadlines.head.saturating_sub(since.elapsed());
        if left.is_zero() {
            return Err(Unread::Close);
        }
        if stream.set_read_timeout(Some(left)).is_err() {
            return Err(Unread::Close);
        }
        match stream.read(&mut piece) {
            Ok(0) | Err(_) => return Err(Unread::Close),
            Ok(read) => buffer.extend_from_slice(&piece[..read]),
        }
    };
    if end > HEAD_LIMIT {
        return Err(Unread::Answer(Response::refused(
            431,
            "a request's line and headers are at most 16 KiB",
        )));
    }
    let rest = buffer.split_off(end + 4);
    buffer.truncate(end);
    let head = parse_head(&buffer).map_err(|why| Unread::Answer(Response::refused(400, why)))?;
    Ok((head, rest))
}

fn parse_head(bytes: &[u8]) -> Result<Head, &'static str> {
    let text = std::str::from_utf8(bytes).map_err(|_| "a request's headers are text")?;
    // Each line ends in CR LF; a bare CR or LF inside one is refused, so that no
    // reader of the headers can see a line this one does not.
    if text.split("\r\n").any(|line| line.contains(['\r', '\n'])) {
        return Err("a request's lines end in CR LF");
    }
    let mut lines = text.split("\r\n");
    let line = lines.next().unwrap_or_default();
    let mut parts = line.split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err("a request line is a method, a path and a version");
    };
    if version != "HTTP/1.1" && version != "HTTP/1.0" {
        return Err("a request is HTTP/1.1");
    }
    if method.is_empty() || !method.bytes().all(|b| b.is_ascii_uppercase()) {
        return Err("a request line is a method, a path and a version");
    }
    if !target.starts_with('/') {
        return Err("a request names a path that starts with /");
    }
    let (path, query) = match target.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (target, None),
    };
    let mut fields = Vec::new();
    for pair in query
        .unwrap_or_default()
        .split('&')
        .filter(|p| !p.is_empty())
    {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        let name = decoded(name).ok_or("a query is fields of text")?;
        let value = decoded(value).ok_or("a query is fields of text")?;
        fields.push((name, value));
    }
    let mut headers: Vec<(String, String)> = Vec::new();
    for line in lines {
        if line.starts_with(' ') || line.starts_with('\t') {
            return Err("a header is one line");
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err("a header is a name and a value");
        };
        if name.is_empty() || !name.bytes().all(is_token) {
            return Err("a header is a name and a value");
        }
        let name = name.to_ascii_lowercase();
        if SINGLE.contains(&name.as_str()) && headers.iter().any(|(named, _)| *named == name) {
            return Err("a header that is sent once was sent twice");
        }
        headers.push((name, value.trim_matches([' ', '\t']).to_owned()));
    }
    if headers.iter().any(|(name, _)| name == "transfer-encoding") {
        return Err("a body is sent with Content-Length, and not in chunks");
    }
    let length = match headers.iter().find(|(name, _)| name == "content-length") {
        None => 0,
        Some((_, value)) => {
            if value.is_empty() || value.len() > 19 || !value.bytes().all(|b| b.is_ascii_digit()) {
                return Err("Content-Length is a number");
            }
            value.parse().map_err(|_| "Content-Length is a number")?
        }
    };
    Ok(Head {
        method: method.to_owned(),
        path: path.to_owned(),
        query: fields,
        headers,
        length,
    })
}

fn is_token(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
}

/// A field of a query with its `%XX` and `+` decoded, when what that gives is text.
fn decoded(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'+' => out.push(b' '),
            b'%' => {
                let high = (*bytes.get(at + 1)? as char).to_digit(16)?;
                let low = (*bytes.get(at + 2)? as char).to_digit(16)?;
                out.push(u8::try_from(high * 16 + low).ok()?);
                at += 2;
            }
            byte => out.push(byte),
        }
        at += 1;
    }
    let text = String::from_utf8(out).ok()?;
    (!text.chars().any(char::is_control)).then_some(text)
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Reads the rest of a body of `length` bytes whose start is `start`: refused, by
/// closing, when it falls behind the rate after its grace or the client goes.
pub fn read_body(
    stream: &mut TcpStream,
    length: u64,
    start: Vec<u8>,
    deadlines: &Deadlines,
) -> Result<Vec<u8>, Unread> {
    let Ok(length) = usize::try_from(length) else {
        return Err(Unread::Close);
    };
    let mut body = start;
    body.truncate(length);
    let since = Instant::now();
    let mut piece = vec![0u8; 64 * 1024];
    let rate = deadlines.rate.max(1);
    while body.len() < length {
        // The moment the body has fallen behind: its grace, and a second for each
        // `rate` bytes received.
        let behind_at =
            deadlines.body_grace + Duration::from_secs_f64(body.len() as f64 / rate as f64);
        let left = behind_at.saturating_sub(since.elapsed());
        if left.is_zero() {
            return Err(Unread::Close);
        }
        let wait = left.min(Duration::from_secs(1));
        if stream.set_read_timeout(Some(wait)).is_err() {
            return Err(Unread::Close);
        }
        let want = (length - body.len()).min(piece.len());
        match stream.read(&mut piece[..want]) {
            Ok(0) => return Err(Unread::Close),
            Ok(read) => body.extend_from_slice(&piece[..read]),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return Err(Unread::Close),
        }
    }
    Ok(body)
}

/// One answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub content_type: &'static str,
    /// Headers beside those every answer carries.
    pub headers: Vec<(&'static str, String)>,
    pub body: Vec<u8>,
}

/// The policy the page is served under (ADR 0041; the plan's "Who may call").
pub const PAGE_POLICY: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' blob: data:; frame-src 'self'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

impl Response {
    /// A JSON answer.
    pub fn json(status: u16, value: &Json) -> Response {
        Response {
            status,
            content_type: "application/json",
            headers: Vec::new(),
            body: value.to_string().into_bytes(),
        }
    }

    /// A refusal: `{"error": why}`, as the page reads one (`ui/src/api.ts`). `why` is in
    /// fixed words, or escaped, and holds nothing the request held.
    pub fn refused(status: u16, why: &str) -> Response {
        Response::json(status, &serde_json::json!({ "error": why }))
    }

    /// Bytes of a type.
    pub fn bytes(status: u16, content_type: &'static str, body: Vec<u8>) -> Response {
        Response {
            status,
            content_type,
            headers: Vec::new(),
            body,
        }
    }

    /// The same, with one more header.
    pub fn with(mut self, name: &'static str, value: &str) -> Response {
        self.headers.push((name, value.to_owned()));
        self
    }

    fn reason(&self) -> &'static str {
        match self.status {
            200 => "OK",
            201 => "Created",
            202 => "Accepted",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            409 => "Conflict",
            413 => "Content Too Large",
            415 => "Unsupported Media Type",
            421 => "Misdirected Request",
            422 => "Unprocessable Content",
            429 => "Too Many Requests",
            431 => "Request Header Fields Too Large",
            501 => "Not Implemented",
            503 => "Service Unavailable",
            _ => "Internal Server Error",
        }
    }

    /// The answer as bytes, with the headers every answer carries.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut head = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\
             Cache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n\
             Referrer-Policy: no-referrer\r\nCross-Origin-Resource-Policy: same-origin\r\n",
            self.status,
            self.reason(),
            self.content_type,
            self.body.len()
        );
        for (name, value) in &self.headers {
            head.push_str(&format!("{name}: {value}\r\n"));
        }
        head.push_str("\r\n");
        let mut bytes = head.into_bytes();
        bytes.extend_from_slice(&self.body);
        bytes
    }

    /// Writes the answer within the answer's deadline, which holds for the whole answer
    /// and not for each write: it is written in pieces, each given what is left of the
    /// deadline, so a client that takes it a few bytes at a time is cut off at the
    /// deadline all the same. Then the connection is closed; what the client sent and
    /// nobody read is drained for a moment first, so that the close does not throw the
    /// answer away.
    pub fn send(&self, stream: &mut TcpStream, deadlines: &Deadlines) {
        let since = Instant::now();
        let bytes = self.to_bytes();
        let mut written = 0;
        while written < bytes.len() {
            let left = deadlines.answer.saturating_sub(since.elapsed());
            if left.is_zero() || stream.set_write_timeout(Some(left)).is_err() {
                return;
            }
            let end = (written + 16 * 1024).min(bytes.len());
            match stream.write(&bytes[written..end]) {
                Ok(0) | Err(_) => return,
                Ok(sent) => written += sent,
            }
        }
        let _ = stream.flush();
        let _ = stream.shutdown(Shutdown::Write);
        let _ = stream.set_read_timeout(Some(Duration::from_millis(50)));
        let mut sink = [0u8; 8192];
        for _ in 0..8 {
            match stream.read(&mut sink) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_parser_refuses_what_could_be_read_two_ways() {
        let refused = |head: &str| parse_head(head.as_bytes()).err();
        let cases: [(&str, Option<&str>); 8] = [
            (
                "GET / HTTP/1.1\r\nHost: a\r\nHost: b",
                Some("a header that is sent once was sent twice"),
            ),
            (
                "POST / HTTP/1.1\r\nTransfer-Encoding: chunked",
                Some("a body is sent with Content-Length, and not in chunks"),
            ),
            (
                "GET / HTTP/1.1\r\nHost: a\r\n folded",
                Some("a header is one line"),
            ),
            (
                "GET / HTTP/1.1\r\nHost: a\nX-Other: b",
                Some("a request's lines end in CR LF"),
            ),
            (
                "GET / HTTP/1.1\r\nContent-Length: 12345678901234567890",
                Some("Content-Length is a number"),
            ),
            (
                "GET / HTTP/1.1\r\nContent-Length: -1",
                Some("Content-Length is a number"),
            ),
            ("GET / HTTP/2\r\nHost: a", Some("a request is HTTP/1.1")),
            ("GET /?a=1 HTTP/1.1\r\nHost: a\r\nContent-Length: 0", None),
        ];
        for (head, expected) in cases {
            assert_eq!(refused(head), expected, "{head:?}");
        }
    }

    #[test]
    fn an_answer_a_client_does_not_take_is_cut_off_at_the_answer_deadline() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        // The client connects, and reads nothing.
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut served, _) = listener.accept().unwrap();
        let deadlines = Deadlines {
            answer: Duration::from_millis(300),
            ..Deadlines::default()
        };
        // Far more than any socket's buffers hold.
        let answer = Response::bytes(200, "application/octet-stream", vec![0u8; 64 << 20]);
        let started = Instant::now();
        answer.send(&mut served, &deadlines);
        let took = started.elapsed();
        assert!(took >= Duration::from_millis(250), "{took:?}");
        assert!(took < Duration::from_secs(5), "{took:?}");
        drop(client);
    }
}
