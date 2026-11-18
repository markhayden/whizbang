//! The HTTP/1.1 the client speaks: the requests it writes, a `GET` and a `POST`, and
//! the head of an answer, read and refused by the rules of the module's top
//! ([`super`]). Nothing here touches a socket; [`super::wire`] does.

use whizbang_core::capability::{Fault, FaultKind};
use whizbang_core::egress::{Address, Credential, Outgoing, Piece};

/// The most bytes of one line of an answer's head, its line ending included.
pub(crate) const LINE_MAX_BYTES: usize = 8 * 1024;

/// The most bytes of an answer's whole head: the status line, the fields and the empty
/// line that ends them.
pub(crate) const HEAD_MAX_BYTES: usize = 64 * 1024;

/// The most fields of an answer's head.
pub(crate) const FIELDS_LIMIT: usize = 100;

/// The request for an address: a `GET` of its path and query, with fixed fields and the
/// credential's field when the request carries one. A credential's value is printable
/// ASCII and spaces and its name is letters, digits, `-` and `_` (ADR 0012), and an
/// address's rest is printable ASCII with no space, so nothing here can end a line.
///
/// It is made in one allocation of its exact size, so that the credential's value is
/// copied once, into what is returned, and into no buffer that grew and was let go:
/// whoever writes it overwrites it then (`client.rs`).
pub(crate) fn request(address: &Address, credential: Option<&Credential>) -> Vec<u8> {
    written(b"GET ", address, &[], credential, None)
}

/// The request that sends `outgoing`: a `POST` of the address's path and query, with
/// the same fixed fields as a `GET`, the request's own fields
/// ([`Outgoing::fields`](whizbang_core::egress::Outgoing::fields), whose names and
/// values the type held to a token and to printable ASCII), its credential's field,
/// and a `Content-Length` of its body; then the body, which is each piece in turn: a
/// picture ([`Piece::is_picture`]) as the base64 of exactly its bytes, and every other
/// piece as it is. Nothing else is written between, before or after them.
///
/// Made in one allocation of its exact size, as [`request`] is.
pub(crate) fn post(outgoing: &Outgoing) -> Vec<u8> {
    written(
        b"POST ",
        outgoing.to(),
        outgoing.fields(),
        outgoing.credential(),
        Some(outgoing.pieces()),
    )
}

/// How many bytes the base64 of `bytes` bytes is, with its padding.
pub(crate) fn base64_len(bytes: usize) -> usize {
    bytes.div_ceil(3) * 4
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Writes the base64 of `bytes` (the standard alphabet, with padding) onto `into`.
pub(crate) fn base64_onto(bytes: &[u8], into: &mut Vec<u8>) {
    let letter = |six: u32| BASE64[(six & 63) as usize];
    for group in bytes.chunks(3) {
        let at = |i: usize| u32::from(group.get(i).copied().unwrap_or(0));
        let all = (at(0) << 16) | (at(1) << 8) | at(2);
        into.push(letter(all >> 18));
        into.push(letter(all >> 12));
        into.push(if group.len() > 1 {
            letter(all >> 6)
        } else {
            b'='
        });
        into.push(if group.len() > 2 { letter(all) } else { b'=' });
    }
}

/// The bytes of a request: the method, the head, and a body when there is one.
fn written(
    method: &[u8],
    address: &Address,
    fields: &[(&str, &str)],
    credential: Option<&Credential>,
    body: Option<&[Piece]>,
) -> Vec<u8> {
    let rest = match address.rest() {
        "" => "/",
        rest => rest,
    };
    let origin = address.origin();
    let port = if origin.port() == 443 {
        String::new()
    } else {
        format!(":{}", origin.port())
    };
    let piece_len = |piece: &Piece| {
        if piece.is_picture() {
            base64_len(piece.bytes().len())
        } else {
            piece.bytes().len()
        }
    };
    let length = body.map(|pieces| {
        let length: usize = pieces.iter().map(piece_len).sum();
        format!("Content-Length: {length}\r\n")
    });
    let parts: [&[u8]; 6] = [
        method,
        rest.as_bytes(),
        b" HTTP/1.1\r\nHost: ",
        origin.host().as_bytes(),
        port.as_bytes(),
        b"\r\nUser-Agent: whizbang\r\nAccept: */*\r\nAccept-Encoding: identity\r\n\
          Connection: close\r\n",
    ];
    let own = fields
        .iter()
        .flat_map(|(name, value)| [name.as_bytes(), b": ", value.as_bytes(), b"\r\n"]);
    let carried: [&[u8]; 4] = match credential {
        Some(credential) => [
            credential.header().as_bytes(),
            b": ",
            credential.expose().as_bytes(),
            b"\r\n",
        ],
        None => [b"", b"", b"", b""],
    };
    let end: [&[u8]; 2] = [length.as_deref().unwrap_or_default().as_bytes(), b"\r\n"];
    let head = || {
        parts
            .into_iter()
            .chain(own.clone())
            .chain(carried)
            .chain(end)
    };
    let body_length: usize = body.map_or(0, |pieces| pieces.iter().map(piece_len).sum());
    let length = head().map(<[u8]>::len).sum::<usize>() + body_length;
    let mut text = Vec::with_capacity(length);
    for part in head() {
        text.extend_from_slice(part);
    }
    for piece in body.unwrap_or_default() {
        if piece.is_picture() {
            base64_onto(piece.bytes(), &mut text);
        } else {
            text.extend_from_slice(piece.bytes());
        }
    }
    text
}

/// An answer's head, as far as the client acts on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Head {
    pub(crate) status: u16,
    fields: Vec<(String, String)>,
}

impl Head {
    /// The values of every field of this name, in order. Names are compared without
    /// regard to case.
    fn all(&self, name: &str) -> impl Iterator<Item = &str> {
        self.fields
            .iter()
            .filter(move |(field, _)| field.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// The one value of a field that may be given once: none when it is absent, a
    /// fault when it is given twice.
    fn once(&self, name: &str, twice: &'static str) -> Result<Option<&str>, Fault> {
        let mut all = self.all(name);
        let first = all.next();
        if all.next().is_some() {
            return Err(malformed(twice));
        }
        Ok(first)
    }

    /// How a send's answer frames its body: by its one `Content-Length`, as
    /// [`Head::length`] reads it, or in chunks, when its one `Transfer-Encoding` is
    /// `chunked` alone and it states no length. Any other transfer coding, a length
    /// beside chunks, and a content coding but `identity` are refused.
    pub(crate) fn framing(&self) -> Result<Framing, Fault> {
        let Some(coding) = self.once("transfer-encoding", CODED)? else {
            return self.length().map(Framing::Length);
        };
        if !coding.eq_ignore_ascii_case("chunked") || self.all("content-length").next().is_some() {
            return Err(malformed(CODED));
        }
        if let Some(coding) = self.once("content-encoding", COMPRESSED)?
            && !coding.eq_ignore_ascii_case("identity")
        {
            return Err(malformed(COMPRESSED));
        }
        Ok(Framing::Chunked)
    }

    /// The length of the body of a `200`: one `Content-Length` of digits alone, no
    /// transfer coding, and no content coding but `identity`.
    pub(crate) fn length(&self) -> Result<u64, Fault> {
        if self.all("transfer-encoding").next().is_some() {
            return Err(malformed(CODED));
        }
        if let Some(coding) = self.once("content-encoding", COMPRESSED)?
            && !coding.eq_ignore_ascii_case("identity")
        {
            return Err(malformed(COMPRESSED));
        }
        let Some(length) = self.once("content-length", LENGTH_TWICE)? else {
            return Err(malformed(NO_LENGTH));
        };
        let digits =
            !length.is_empty() && length.len() <= 19 && length.bytes().all(|b| b.is_ascii_digit());
        if !digits {
            return Err(malformed(BAD_LENGTH));
        }
        length.parse().map_err(|_| malformed(BAD_LENGTH))
    }

    /// Where a redirect points: its one `Location`.
    pub(crate) fn location(&self) -> Result<&str, Fault> {
        match self.once("location", LOCATION_TWICE)? {
            Some(location) if !location.is_empty() => Ok(location),
            _ => Err(malformed(NO_LOCATION)),
        }
    }
}

/// How a send's answer frames its body ([`Head::framing`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Framing {
    Length(u64),
    Chunked,
}

/// The most bytes of a chunk's size line, its extensions and its ending included.
pub(crate) const CHUNK_LINE_MAX_BYTES: usize = 256;

/// The most lines of trailer after the last chunk; each is at most [`LINE_MAX_BYTES`].
pub(crate) const TRAILERS_LIMIT: usize = 16;

pub(crate) const CHUNK_LINE: &str = "a chunk's size line of the answer is not hex digits, with \
                                     extensions, within 256 bytes";
pub(crate) const CHUNK_END: &str = "a chunk of the answer does not end with a line ending";
pub(crate) const CHUNKED_TOO_LARGE: &str = "the answer's chunks hold more than the most a \
                                            reply holds";
pub(crate) const TRAILERS: &str = "the answer's trailer is over 16 lines or a line of it over \
                                   8 KiB";

/// The bytes of a chunked body as they come: those that came with the head, and then
/// what `more` reads, which answers how many bytes it put in the buffer it is handed
/// and none only when the connection ended.
pub(crate) struct Chunks<'a> {
    held: Vec<u8>,
    at: usize,
    more: &'a mut dyn FnMut(&mut [u8]) -> Result<usize, Fault>,
}

impl<'a> Chunks<'a> {
    pub(crate) fn new(
        start: Vec<u8>,
        more: &'a mut dyn FnMut(&mut [u8]) -> Result<usize, Fault>,
    ) -> Chunks<'a> {
        Chunks {
            held: start,
            at: 0,
            more,
        }
    }

    /// Reads on, once; a connection that ends here ends the body before it is whole.
    fn fill(&mut self) -> Result<(), Fault> {
        if self.at > 0 {
            self.held.drain(..self.at);
            self.at = 0;
        }
        let mut buffer = [0u8; 16 * 1024];
        let read = (self.more)(&mut buffer)?;
        if read == 0 {
            return Err(Fault::new(FaultKind::Malformed, super::wire::CUT).unresolved());
        }
        self.held.extend_from_slice(&buffer[..read]);
        Ok(())
    }

    /// One line, without its ending, of at most `most` bytes with the ending. A bare
    /// carriage return or line feed in it is refused.
    fn line(&mut self, most: usize, why: &'static str) -> Result<Vec<u8>, Fault> {
        loop {
            let left = &self.held[self.at..];
            if let Some(end) = left.windows(2).position(|pair| pair == b"\r\n") {
                if end + 2 > most {
                    return Err(malformed(why));
                }
                let line = left[..end].to_vec();
                if line.iter().any(|&b| b == b'\r' || b == b'\n') {
                    return Err(malformed(BARE_ENDING));
                }
                self.at += end + 2;
                return Ok(line);
            }
            if left.len() >= most {
                return Err(malformed(why));
            }
            self.fill()?;
        }
    }

    /// Exactly `count` bytes, onto `into`.
    fn take(&mut self, mut count: usize, into: &mut Vec<u8>) -> Result<(), Fault> {
        while count > 0 {
            if self.at == self.held.len() {
                self.fill()?;
            }
            let here = (self.held.len() - self.at).min(count);
            into.extend_from_slice(&self.held[self.at..self.at + here]);
            self.at += here;
            count -= here;
        }
        Ok(())
    }

    /// The whole body: each chunk's size line (hex digits, then any extensions after a
    /// `;`, which are not read), its bytes and its line ending, until the chunk of
    /// size nothing; then the trailer's lines, at most [`TRAILERS_LIMIT`], which are
    /// not read either, and the empty line. At most `most` bytes in all.
    pub(crate) fn body(mut self, most: usize) -> Result<Vec<u8>, Fault> {
        let mut body = Vec::new();
        loop {
            let line = self.line(CHUNK_LINE_MAX_BYTES, CHUNK_LINE)?;
            let digits = line.split(|&b| b == b';').next().unwrap_or_default();
            let hex =
                !digits.is_empty() && digits.len() <= 8 && digits.iter().all(u8::is_ascii_hexdigit);
            if !hex {
                return Err(malformed(CHUNK_LINE));
            }
            let size = digits.iter().fold(0usize, |size, &digit| {
                let value = char::from(digit).to_digit(16).unwrap_or(0) as usize;
                size * 16 + value
            });
            if size == 0 {
                break;
            }
            if body.len().saturating_add(size) > most {
                return Err(Fault::new(FaultKind::TooLarge, CHUNKED_TOO_LARGE).unresolved());
            }
            self.take(size, &mut body)?;
            let mut ending = Vec::new();
            self.take(2, &mut ending)?;
            if ending != b"\r\n" {
                return Err(malformed(CHUNK_END));
            }
        }
        for _ in 0..=TRAILERS_LIMIT {
            if self.line(LINE_MAX_BYTES, TRAILERS)?.is_empty() {
                return Ok(body);
            }
        }
        Err(malformed(TRAILERS))
    }
}

/// What the client answers of an answer it cannot read. Each is fixed words: no fault
/// quotes a byte of what came.
pub(crate) const NOT_HTTP: &str = "the answer does not start with the status line of HTTP 1.1";
pub(crate) const INTERIM: &str = "the answer is an interim one, 1xx, which the client does not \
                                  handle";
pub(crate) const LINE_TOO_LONG: &str = "a line of the answer's head is over 8 KiB";
pub(crate) const HEAD_TOO_LARGE: &str = "the answer's head is over 64 KiB";
pub(crate) const TOO_MANY_FIELDS: &str = "the answer's head holds more than 100 fields";
pub(crate) const BARE_ENDING: &str = "the answer's head holds a carriage return or a line feed \
                                      that is not part of a line ending";
pub(crate) const FOLDED: &str = "the answer's head holds a folded line, which the client does \
                                 not handle";
pub(crate) const BAD_FIELD: &str = "a field of the answer's head is not a name, a colon and a \
                                    value";
pub(crate) const CODED: &str = "the answer's body has a transfer coding, chunked or another, \
                                which the client does not handle";
pub(crate) const COMPRESSED: &str = "the answer's body is compressed, which the client did not \
                                     ask for";
pub(crate) const NO_LENGTH: &str = "the answer states no length for its body";
pub(crate) const LENGTH_TWICE: &str = "the answer states the length of its body more than once";
pub(crate) const BAD_LENGTH: &str = "the answer's length is not a number of digits alone";
pub(crate) const NO_LOCATION: &str = "the answer points elsewhere and names no address";
pub(crate) const LOCATION_TWICE: &str = "the answer points elsewhere and names more than one \
                                         address";

fn malformed(why: &'static str) -> Fault {
    Fault::new(FaultKind::Malformed, why).unresolved()
}

/// Where the head ends in `bytes`, after its empty line, when it is all there. A head
/// longer than [`HEAD_MAX_BYTES`] without its end is refused.
///
/// `scanned` is how many of the bytes were looked at before, by a call that found no
/// end: the search starts three bytes before that, where an end that the new bytes
/// complete may begin, so each byte is looked at a bounded number of times however
/// the head arrives.
pub(crate) fn end_of_head(bytes: &[u8], scanned: usize) -> Result<Option<usize>, Fault> {
    let from = scanned.saturating_sub(3).min(bytes.len());
    let end = bytes[from..]
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|at| from + at + 4);
    match end {
        Some(end) if end <= HEAD_MAX_BYTES => Ok(Some(end)),
        Some(_) => Err(malformed(HEAD_TOO_LARGE)),
        None if bytes.len() >= HEAD_MAX_BYTES => Err(malformed(HEAD_TOO_LARGE)),
        None => {
            // A line that is already over its bound is refused before the rest comes.
            // Only the last line's worth of bytes is looked at for where it starts.
            let tail = bytes.len().saturating_sub(LINE_MAX_BYTES + 2);
            let last = bytes[tail..]
                .windows(2)
                .rposition(|window| window == b"\r\n")
                .map_or(tail, |at| tail + at + 2);
            if bytes.len() - last > LINE_MAX_BYTES {
                return Err(malformed(LINE_TOO_LONG));
            }
            Ok(None)
        }
    }
}

/// The head, from its bytes up to and with its empty line.
pub(crate) fn head(bytes: &[u8]) -> Result<Head, Fault> {
    let Some(text) = bytes.strip_suffix(b"\r\n\r\n") else {
        return Err(malformed(NOT_HTTP));
    };
    let mut lines = Vec::new();
    let mut rest = text;
    loop {
        let (line, after) = match rest.windows(2).position(|window| window == b"\r\n") {
            Some(at) => (&rest[..at], Some(&rest[at + 2..])),
            None => (rest, None),
        };
        if line.len() + 2 > LINE_MAX_BYTES {
            return Err(malformed(LINE_TOO_LONG));
        }
        if line.iter().any(|&b| b == b'\r' || b == b'\n') {
            return Err(malformed(BARE_ENDING));
        }
        lines.push(line);
        match after {
            Some(after) => rest = after,
            None => break,
        }
    }
    let (status_line, field_lines) = lines.split_first().ok_or_else(|| malformed(NOT_HTTP))?;
    let status = status(status_line)?;
    if field_lines.len() > FIELDS_LIMIT {
        return Err(malformed(TOO_MANY_FIELDS));
    }
    let fields = field_lines
        .iter()
        .map(|line| field(line))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Head { status, fields })
}

/// `HTTP/1.1` or `HTTP/1.0`, a space, three digits, a space, and a reason of any
/// visible characters and spaces, which is not read.
fn status(line: &[u8]) -> Result<u16, Fault> {
    let version = line.get(..9);
    if version != Some(b"HTTP/1.1 ") && version != Some(b"HTTP/1.0 ") {
        return Err(malformed(NOT_HTTP));
    }
    let digits = line.get(9..12).ok_or_else(|| malformed(NOT_HTTP))?;
    if !digits.iter().all(u8::is_ascii_digit) || digits[0] == b'0' {
        return Err(malformed(NOT_HTTP));
    }
    let after = line.get(12..).unwrap_or_default();
    if !after.is_empty() && after[0] != b' ' {
        return Err(malformed(NOT_HTTP));
    }
    let reason_ok = after
        .iter()
        .all(|&b| b == b' ' || b == b'\t' || b.is_ascii_graphic());
    if !reason_ok {
        return Err(malformed(NOT_HTTP));
    }
    let status = digits
        .iter()
        .fold(0u16, |n, &d| n * 10 + u16::from(d - b'0'));
    if status < 200 {
        return Err(malformed(INTERIM));
    }
    Ok(status)
}

/// A token, a colon, optional white space, and a value of visible characters, spaces
/// and tabs, which loses the white space at its ends.
fn field(line: &[u8]) -> Result<(String, String), Fault> {
    if line.first().is_some_and(|&b| b == b' ' || b == b'\t') {
        return Err(malformed(FOLDED));
    }
    let colon = line
        .iter()
        .position(|&b| b == b':')
        .ok_or_else(|| malformed(BAD_FIELD))?;
    let (name, value) = (&line[..colon], &line[colon + 1..]);
    let token = |b: &u8| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(b);
    if name.is_empty() || !name.iter().all(token) {
        return Err(malformed(BAD_FIELD));
    }
    if !value
        .iter()
        .all(|&b| b == b' ' || b == b'\t' || b.is_ascii_graphic())
    {
        return Err(malformed(BAD_FIELD));
    }
    let value = value.trim_ascii();
    // Both are ASCII by now.
    Ok((
        String::from_utf8_lossy(name).into_owned(),
        String::from_utf8_lossy(value).into_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use whizbang_core::contract::Secrets;
    use whizbang_core::egress::Origin;

    fn address(text: &str) -> Address {
        Address::of(text, &Secrets::none()).unwrap()
    }

    fn base64(bytes: &[u8]) -> String {
        let mut out = Vec::new();
        base64_onto(bytes, &mut out);
        assert_eq!(out.len(), base64_len(bytes.len()));
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn base64_is_the_standard_alphabet_with_its_padding() {
        // The vectors of RFC 4648, section 10.
        let vectors = [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ];
        for (plain, encoded) in vectors {
            assert_eq!(base64(plain.as_bytes()), encoded, "{plain}");
        }
        assert_eq!(base64(&[0xfb, 0xff, 0xbf]), "+/+/");
    }

    #[test]
    fn a_post_is_its_head_its_own_fields_the_credential_its_length_and_its_pieces_in_order() {
        use whizbang_core::capability::Picture;
        use whizbang_core::egress::DataClass;
        use whizbang_core::identity::CanonicalPixels;
        let pixels = CanonicalPixels::from_rgba(1, 1, vec![1, 2, 3, 255]).unwrap();
        let picture =
            Piece::picture(DataClass::SourceImages, &Picture::new(pixels).unwrap()).unwrap();
        let encoded = base64(picture.bytes());
        let pieces = vec![
            Piece::of(DataClass::Prompts, b"{\"a\":\"".to_vec()),
            picture,
            Piece::of(DataClass::Prompts, b"\"}".to_vec()),
        ];
        let to = address("https://127.0.0.1:8443/v1/messages");
        let origin = Origin::of("https://127.0.0.1:8443", &Secrets::none()).unwrap();
        let credential =
            Credential::new(origin, "x-api-key", "made-up-value", &Secrets::none()).unwrap();
        let outgoing = Outgoing::new(to, pieces)
            .unwrap()
            .with_fields(&[("content-type", "application/json"), ("x-version", "1")])
            .unwrap()
            .with_credential(credential)
            .unwrap();
        let written = post(&outgoing);
        // Made in one allocation of its size: no larger buffer held the value first.
        assert_eq!(written.capacity(), written.len());
        let body = format!("{{\"a\":\"{encoded}\"}}");
        let expected = format!(
            "POST /v1/messages HTTP/1.1\r\nHost: 127.0.0.1:8443\r\nUser-Agent: whizbang\r\n\
             Accept: */*\r\nAccept-Encoding: identity\r\nConnection: close\r\n\
             content-type: application/json\r\nx-version: 1\r\n\
             x-api-key: made-up-value\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        assert_eq!(String::from_utf8(written).unwrap(), expected);
    }

    #[test]
    fn the_request_is_a_get_with_fixed_fields_and_the_credential_alone_beside_them() {
        let bare = request(&address("https://weights.example.org/a/b?c=d"), None);
        assert_eq!(
            bare,
            b"GET /a/b?c=d HTTP/1.1\r\nHost: weights.example.org\r\nUser-Agent: whizbang\r\n\
              Accept: */*\r\nAccept-Encoding: identity\r\nConnection: close\r\n\r\n"
        );
        let origin = Origin::of("https://127.0.0.1:8443", &Secrets::none()).unwrap();
        let credential =
            Credential::new(origin, "Authorization", "Bearer made-up", &Secrets::none()).unwrap();
        let carried = request(&address("https://127.0.0.1:8443"), Some(&credential));
        // Made in one allocation of its size: no larger buffer held the value first.
        assert_eq!(carried.capacity(), carried.len());
        assert_eq!(
            carried,
            b"GET / HTTP/1.1\r\nHost: 127.0.0.1:8443\r\nUser-Agent: whizbang\r\n\
              Accept: */*\r\nAccept-Encoding: identity\r\nConnection: close\r\n\
              Authorization: Bearer made-up\r\n\r\n"
        );
    }

    #[test]
    fn a_head_is_read_to_its_status_and_fields() {
        let read = head(b"HTTP/1.1 200 OK\r\nContent-Length: 12\r\nX-A:b\r\n\r\n").unwrap();
        assert_eq!(read.status, 200);
        assert_eq!(read.length().unwrap(), 12);
        let moved = head(b"HTTP/1.0 302 Found\r\nlocation:  /there \r\n\r\n").unwrap();
        assert_eq!(moved.location().unwrap(), "/there");
        // A status line with no reason is one.
        assert_eq!(head(b"HTTP/1.1 200\r\n\r\n").unwrap().status, 200);
    }

    #[test]
    fn the_end_of_a_head_is_found_and_a_line_or_a_head_over_its_bound_is_refused_before_its_end() {
        assert_eq!(
            end_of_head(b"HTTP/1.1 200 OK\r\n\r\nbody", 0).unwrap(),
            Some(19)
        );
        assert_eq!(end_of_head(b"HTTP/1.1 200 OK\r\nA: b", 0).unwrap(), None);
        let long_line = [
            b"HTTP/1.1 200 OK\r\nA: ".as_slice(),
            &[b'b'; LINE_MAX_BYTES],
        ]
        .concat();
        let refused = end_of_head(&long_line, 0).unwrap_err();
        assert_eq!(refused.sentence(), LINE_TOO_LONG);
        let many = b"A: b\r\n".repeat(HEAD_MAX_BYTES / 6 + 1);
        assert_eq!(
            end_of_head(&many, 0).unwrap_err().sentence(),
            HEAD_TOO_LARGE
        );
    }

    #[test]
    fn the_end_of_a_head_that_comes_a_byte_at_a_time_is_found_from_where_the_last_look_stopped() {
        // Each byte is added and only what is new, and the three bytes before it, are
        // looked at: the end is found at the byte that completes it, wherever the
        // earlier bytes put it.
        let whole = b"HTTP/1.1 200 OK\r\nA: b\r\n\r\nbody";
        let mut found = None;
        for length in 1..=whole.len() {
            if let Some(end) = end_of_head(&whole[..length], length - 1).unwrap() {
                found = Some((length, end));
                break;
            }
        }
        assert_eq!(found, Some((25, 25)));
        // Looked at from past an end it would have found, it does not find it: the
        // search starts where it is told to.
        assert_eq!(
            end_of_head(b"HTTP/1.1 200 OK\r\n\r\nbody", 10).unwrap(),
            Some(19)
        );
        assert_eq!(
            end_of_head(b"HTTP/1.1 200 OK\r\n\r\nbody", 20).unwrap(),
            None
        );
    }
}
