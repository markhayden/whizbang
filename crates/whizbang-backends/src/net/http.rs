//! The HTTP/1.1 the download client speaks: the request it writes, and the head of an
//! answer, read and refused by the rules of the module's top ([`super`]). Nothing here
//! touches a socket; [`super::wire`] does.

use whizbang_core::capability::{Fault, FaultKind};
use whizbang_core::egress::{Address, Credential};

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
    let parts: [&[u8]; 6] = [
        b"GET ",
        rest.as_bytes(),
        b" HTTP/1.1\r\nHost: ",
        origin.host().as_bytes(),
        port.as_bytes(),
        b"\r\nUser-Agent: whizbang\r\nAccept: */*\r\nAccept-Encoding: identity\r\n\
          Connection: close\r\n",
    ];
    let carried: [&[u8]; 4] = match credential {
        Some(credential) => [
            credential.header().as_bytes(),
            b": ",
            credential.expose().as_bytes(),
            b"\r\n",
        ],
        None => [b"", b"", b"", b""],
    };
    let end: [&[u8]; 1] = [b"\r\n"];
    let all = parts.iter().chain(&carried).chain(&end);
    let length = all.clone().map(|part| part.len()).sum();
    let mut text = Vec::with_capacity(length);
    for part in all {
        text.extend_from_slice(part);
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
