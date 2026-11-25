//! How an answer of the front is read (ADR 0046, "Its routes", "The outcome record",
//! "The identity", "The headers at once"): its head first, which waits for nothing on
//! the host; then its body, where keep-alives are passed over, the
//! identity record names the files the front checked, the frames follow it through
//! `pack/1`'s own codec under the bounds of the requests they answer, and the outcome
//! record ends it under its own bounds. Every length is read and checked before its
//! bytes, and nothing of an answer is used before its identity is held to the
//! catalogue's: a frame before the identity record refuses the answer whole.

use std::io::Read;

use serde_json::Value;
use whizbang_core::capability::{Fault, FaultKind};
use whizbang_core::contract::Secrets;
use whizbang_core::egress::Head;

use super::Body;
use crate::pack::proto::{self, Answer, Awaited, Heard, Message, Open, Reason, Role};
use crate::pack::{Called, Ended, Failed};

/// The ids of the two requests of every request of the front: its `load` and its call.
pub(crate) const LOAD_ID: &str = "r1";
pub(crate) const CALL_ID: &str = "r2";

/// The outcome record's mark: four zero bytes, which no frame begins with.
const MARK: [u8; 4] = [0; 4];
/// The most bytes of the record's document.
pub const RECORD_MAX_BYTES: u32 = 4 * 1024;
/// The most lines of error output a record holds, and the most bytes of one.
pub const LINES_MOST: u32 = 256;
pub const LINE_MAX_BYTES: u32 = 4_096;
/// The most keep-alives one answer holds: the front sends one after 10 seconds of
/// nothing else, so this is more than 11 hours of them, far past any host's limit; it
/// bounds an answer that is kept alive and never ends, whatever the run's deadline.
pub const KEEP_ALIVES_MOST: u32 = 4_096;

pub const TRUNCATED: &str = "the front's answer ended before its outcome record, or a length \
                             in it is over its bound: it is truncated";
pub const NO_RESULT: &str = "the front's answer holds no result of the call and no failure";
pub const REDIRECTED: &str = "the front pointed the request elsewhere, which is refused, and \
                              nothing follows it";
pub const CREDENTIAL: &str = "the front refused the request's token: 401 or 403";
pub const BUSY: &str = "the front could not take the request now: 503";
pub const STATUS: &str = "the front answered with a status the kind does not act on";
pub const NO_IDENTITY: &str = "the front's identity record names no digest of the files it \
                               came from, or a malformed one, and the answer is refused whole";
pub const OTHER_IDENTITY: &str = "the front's answer names the files of another model than the \
                                  catalogue's, and is refused whole";
pub const FRAMES_UNVERIFIED: &str = "the front's answer holds a frame before its identity \
                                     record, and is refused whole";
pub const KEPT_ALIVE: &str = "the front's answer holds more keep-alives than an answer may, \
                              and is refused";
pub const MALFORMED_RECORD: &str = "the front's outcome record is not one of pack-host/1";
pub const OUT_OF_PLACE: &str = "the front's answer holds a frame that has no place in it";

/// What an answer came to: the call's result, or its failure as the record told it.
pub(crate) enum Came {
    Called(Called),
    Failed(Box<Failed>),
}

/// A reader that gives back the bytes looked at first, then the rest.
struct Peeked<'a> {
    first: [u8; 4],
    at: usize,
    rest: &'a mut dyn Read,
}

impl Read for Peeked<'_> {
    fn read(&mut self, into: &mut [u8]) -> std::io::Result<usize> {
        if self.at < self.first.len() {
            let here = (self.first.len() - self.at).min(into.len());
            into[..here].copy_from_slice(&self.first[self.at..self.at + here]);
            self.at += here;
            return Ok(here);
        }
        self.rest.read(into)
    }
}

fn truncated() -> Fault {
    Fault::new(FaultKind::Unavailable, TRUNCATED).unresolved()
}

fn malformed(why: &str) -> Fault {
    Fault::new(FaultKind::Malformed, why).unresolved()
}

/// Exactly `count` bytes, or the answer is truncated.
fn exactly(input: &mut dyn Read, count: usize) -> Result<Vec<u8>, Fault> {
    let mut bytes = vec![0; count];
    input.read_exact(&mut bytes).map_err(|_| truncated())?;
    Ok(bytes)
}

/// The next four bytes, or none when the body has ended before them.
fn next_four(input: &mut dyn Read) -> Result<Option<[u8; 4]>, Fault> {
    let mut four = [0u8; 4];
    let mut filled = 0;
    while filled < 4 {
        match input.read(&mut four[filled..]) {
            Ok(0) if filled == 0 => return Ok(None),
            Ok(0) => return Err(truncated()),
            Ok(read) => filled += read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return Err(truncated()),
        }
    }
    Ok(Some(four))
}

/// The digest's text, when it is `sha256:` and 64 lower-case hex digits.
fn is_digest(text: &str) -> bool {
    text.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// What a frame the codec refused ends the answer as.
fn refused(refused: &proto::Refused) -> Fault {
    match refused.reason() {
        Reason::Truncated => truncated(),
        Reason::HeaderTooLarge | Reason::OverBound | Reason::TooLarge => Fault::new(
            FaultKind::TooLarge,
            &format!(
                "the front's answer holds a frame over a bound of pack/1 ({}), refused before it was held",
                refused.reason().name()
            ),
        )
        .unresolved(),
        reason => malformed(&format!(
            "the front's answer holds a frame pack/1 refuses ({})",
            reason.name()
        )),
    }
}

/// Reads one answer of the front: its head, its frames and its outcome record.
pub(crate) fn read(
    head: &Head,
    input: &mut dyn Read,
    digest: &str,
    body: &Body,
    secrets: &Secrets,
) -> Result<Came, Fault> {
    match head.status() {
        200 => {}
        301 | 302 | 303 | 307 | 308 => {
            return Err(Fault::new(FaultKind::Rejected, REDIRECTED).unresolved());
        }
        401 | 403 => return Err(Fault::new(FaultKind::Credential, CREDENTIAL).unresolved()),
        503 => return Err(Fault::new(FaultKind::Unavailable, BUSY).unresolved()),
        status => {
            return Err(malformed(&format!("{STATUS}: {status}")));
        }
    }
    frames(input, digest, body, secrets)
}

/// What follows a mark.
enum Marked {
    /// A keep-alive: the mark and a length of 0.
    KeepAlive,
    /// The identity record, its digest held to the catalogue's.
    Identity,
    /// The outcome record, read to the end of the answer.
    Outcome(Record),
}

/// What a mark begins, its length read and held to its bound before its document is
/// read; an identity record's digest held to `digest`.
fn marked(input: &mut dyn Read, digest: &str, secrets: &Secrets) -> Result<Marked, Fault> {
    let size = length(input)?;
    if size == 0 {
        return Ok(Marked::KeepAlive);
    }
    if size > RECORD_MAX_BYTES {
        return Err(truncated());
    }
    let document = exactly(input, size as usize)?;
    let document: Value =
        serde_json::from_slice(&document).map_err(|_| malformed(MALFORMED_RECORD))?;
    if document.get("files").is_some() {
        let named = match &document {
            Value::Object(fields)
                if fields.len() == 2
                    && fields.get("format").and_then(Value::as_str) == Some("pack-host/1") =>
            {
                fields.get("files").and_then(Value::as_str)
            }
            _ => None,
        };
        let named = named
            .filter(|named| is_digest(named))
            .ok_or_else(|| malformed(NO_IDENTITY))?;
        if named != digest {
            return Err(Fault::new(FaultKind::Rejected, OTHER_IDENTITY).unresolved());
        }
        return Ok(Marked::Identity);
    }
    record(&document, input, secrets).map(Marked::Outcome)
}

/// Where the reading of the frames is.
#[derive(PartialEq, Eq)]
enum At {
    Hello,
    Load,
    Call,
    Done,
}

/// The body of an answer: its keep-alives, its identity record and its frames, then its
/// outcome record.
fn frames(
    input: &mut dyn Read,
    digest: &str,
    body: &Body,
    secrets: &Secrets,
) -> Result<Came, Fault> {
    let mut open = Open::new();
    open.expect(LOAD_ID, Awaited::of(&body.load))
        .map_err(|_| malformed(OUT_OF_PLACE))?;
    open.expect(CALL_ID, Awaited::of(&body.call))
        .map_err(|_| malformed(OUT_OF_PLACE))?;
    let mut at = At::Hello;
    let mut verified = false;
    let mut kept_alive: u32 = 0;
    let mut events = Vec::new();
    // The card the `load` named, when it named one (ADR 0048).
    let mut card = None;
    let mut ended: Option<Result<Called, (FaultKind, String)>> = None;
    loop {
        let Some(first) = next_four(input)? else {
            return Err(truncated());
        };
        if first == MARK {
            let record = match marked(input, digest, secrets)? {
                Marked::KeepAlive => {
                    kept_alive += 1;
                    if kept_alive > KEEP_ALIVES_MOST {
                        return Err(malformed(KEPT_ALIVE));
                    }
                    continue;
                }
                // One identity, before any frame.
                Marked::Identity if !verified && at == At::Hello => {
                    verified = true;
                    continue;
                }
                Marked::Identity => return Err(malformed(OUT_OF_PLACE)),
                Marked::Outcome(record) => record,
            };
            return match ended {
                Some(Ok(called)) if record.complete => Ok(Came::Called(called)),
                Some(Ok(_)) => Err(malformed(MALFORMED_RECORD)),
                Some(Err((kind, said))) => {
                    let mut failed = record.failed(Some(kind))?;
                    failed.said = Some(said);
                    Ok(Came::Failed(failed))
                }
                None if !record.complete => record.failed(None).map(Came::Failed),
                None => Err(malformed(NO_RESULT)),
            };
        }
        if !verified {
            return Err(malformed(FRAMES_UNVERIFIED));
        }
        if at == At::Done {
            return Err(malformed(OUT_OF_PLACE));
        }
        let mut peeked = Peeked {
            first,
            at: 0,
            rest: &mut *input,
        };
        let heard = proto::hear(&mut peeked, &mut open).map_err(|refusal| refused(&refusal))?;
        let message = match heard {
            Some(Heard::Message(message)) => message,
            Some(Heard::Asked { .. }) | None => return Err(malformed(OUT_OF_PLACE)),
        };
        match (&at, message) {
            (
                At::Hello,
                Message::Hello {
                    id,
                    role: Role::Worker { .. },
                },
            ) if id == body.conversation => {
                at = At::Load;
            }
            (At::Load, Message::Event { id, .. }) if id == LOAD_ID => {}
            (
                At::Load,
                Message::Result {
                    id,
                    answer: Answer::Load,
                    diagnostics,
                    ..
                },
            ) if id == LOAD_ID => {
                card = crate::pack::card_of(&diagnostics);
                at = At::Call;
            }
            (
                At::Load,
                Message::Error {
                    id,
                    fault,
                    sentence,
                },
            ) if id == LOAD_ID => {
                ended = Some(Err((fault, sentence)));
                at = At::Done;
            }
            (At::Call, Message::Event { id, text }) if id == CALL_ID => events.push(text),
            (
                At::Call,
                Message::Result {
                    id,
                    device,
                    threads,
                    diagnostics,
                    answer,
                },
            ) if id == CALL_ID => {
                ended = Some(Ok(Called {
                    device,
                    threads,
                    diagnostics,
                    events: std::mem::take(&mut events),
                    answer,
                    card: card.take(),
                }));
                at = At::Done;
            }
            (
                At::Call,
                Message::Error {
                    id,
                    fault,
                    sentence,
                },
            ) if id == CALL_ID => {
                ended = Some(Err((fault, sentence)));
                at = At::Done;
            }
            _ => return Err(malformed(OUT_OF_PLACE)),
        }
    }
}

/// An outcome record, read.
struct Record {
    complete: bool,
    kind: Option<FaultKind>,
    admission: Option<String>,
    ended: Option<Ended>,
    lines: Vec<String>,
}

impl Record {
    /// The failure this record tells, of the kind the worker's error gave when it gave
    /// one: its fault (whose stage is set by whoever sent the request, never read from
    /// the record), how the worker ended, and its error output.
    fn failed(self, kind: Option<FaultKind>) -> Result<Box<Failed>, Fault> {
        if self.complete && kind.is_none() {
            return Err(malformed(NO_RESULT));
        }
        let kind = kind
            .or(self.kind)
            .ok_or_else(|| malformed(MALFORMED_RECORD))?;
        let mut sentence = format!("the front ended the request with the fault {}", kind.name());
        if let Some(admission) = &self.admission {
            sentence.push_str(&format!(", refused at its admission: {admission}"));
        }
        if let Some(ended) = self.ended {
            sentence.push_str(match ended {
                Ended::Exited(_) => "; its worker ended by itself",
                Ended::Killed => "; its worker was killed",
                Ended::Signalled => "; a signal ended its worker",
            });
        }
        Ok(Box::new(Failed {
            fault: Fault::new(kind, &sentence).unresolved(),
            said: None,
            ended: self.ended,
            error_output: self.lines,
        }))
    }
}

/// A whole number of four bytes.
fn length(input: &mut dyn Read) -> Result<u32, Fault> {
    let bytes = exactly(input, 4)?;
    Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

/// The outcome record, after its mark and its document: its lines, each length read and
/// held to its bound before its bytes are; and then the end of the answer.
fn record(document: &Value, input: &mut dyn Read, secrets: &Secrets) -> Result<Record, Fault> {
    let text = |key: &str| document.get(key).and_then(Value::as_str);
    if text("format") != Some("pack-host/1") {
        return Err(malformed(MALFORMED_RECORD));
    }
    let complete = match text("outcome") {
        Some("complete") => true,
        Some("failed") => false,
        _ => return Err(malformed(MALFORMED_RECORD)),
    };
    let kind = match document.get("kind") {
        None | Some(Value::Null) => None,
        Some(Value::String(name)) => Some(
            FaultKind::ALL
                .into_iter()
                .find(|kind| kind.name() == name)
                .ok_or_else(|| malformed(MALFORMED_RECORD))?,
        ),
        Some(_) => return Err(malformed(MALFORMED_RECORD)),
    };
    let admission = match document.get("admission") {
        None | Some(Value::Null) => None,
        Some(Value::String(word))
            if ["busy", "replay", "not_after", "claim_unreachable"].contains(&word.as_str()) =>
        {
            Some(word.clone())
        }
        Some(_) => return Err(malformed(MALFORMED_RECORD)),
    };
    let ended = match document.get("ended") {
        None | Some(Value::Null) => None,
        Some(Value::String(word)) if word == "killed" => Some(Ended::Killed),
        Some(Value::String(word)) if word == "signalled" => Some(Ended::Signalled),
        Some(Value::Object(exited)) => {
            let code = exited
                .get("exited")
                .and_then(Value::as_i64)
                .and_then(|code| i32::try_from(code).ok())
                .ok_or_else(|| malformed(MALFORMED_RECORD))?;
            Some(Ended::Exited(code))
        }
        Some(_) => return Err(malformed(MALFORMED_RECORD)),
    };
    let count = document
        .get("lines")
        .and_then(Value::as_u64)
        .ok_or_else(|| malformed(MALFORMED_RECORD))?;
    if count > u64::from(LINES_MOST) {
        return Err(truncated());
    }
    let mut lines = Vec::new();
    for _ in 0..count {
        let size = length(input)?;
        if size > LINE_MAX_BYTES {
            return Err(truncated());
        }
        let line = exactly(input, size as usize)?;
        lines.push(secrets.redact(&String::from_utf8_lossy(&line)));
    }
    let mut after = [0u8; 1];
    match input.read(&mut after) {
        Ok(0) => {}
        Ok(_) => return Err(malformed(MALFORMED_RECORD)),
        Err(_) => return Err(truncated()),
    }
    Ok(Record {
        complete,
        kind,
        admission,
        ended,
        lines,
    })
}
