//! A judge's turns and exchanges as data (ADR 0009): written as JSON, and read back
//! through the checked constructors of the judge's types and in no other way.
//!
//! Two documents, each with its kind and version in its first field:
//!
//! - a **script** (`judge-script/1`): the answers a scripted judge plays, in order, each
//!   a turn or a fault. It is what a test, or a replay, reads its judge from;
//! - an **exchange** (`judge-exchange/1`): one request to a judge and what came back.
//!   It is what a run keeps of each request, as a diagnostic.
//!
//! The types of a conversation have no `Deserialize`, and gain none here: what is read
//! is a private mirror of the document, and each value is then made by the constructor
//! that checks it. So data that breaks a bound of a turn, a message or a request is
//! refused when it is read, with the type's own refusal.
//!
//! Reading is strict and bounded: the bytes are counted before they are parsed, a
//! field that is not known is refused, and so is anything after the document.
//!
//! **An image** is written as its size and its canonical pixel digest, never as
//! pixels. Reading a document that holds one asks the caller for the picture.
//!
//! **An opaque part** is written in one of two ways, which the writer chooses
//! ([`Keep`]). Whole, with its body, byte for byte: for a script, which must hand the
//! part to a judge loop as a provider would. Or as its adapter, the digest of its body
//! and its length, with no body: for what a run stores, since a body cannot be redacted
//! and stay the part the provider sent. A part written without its body cannot be read
//! back as a part, and reading says so ([`Unread::Withheld`]).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::identity::{Digest, PixelDigest};

use super::{
    Controls, Fault, FaultKind, JudgeRequest, Message, Opaque, Part, Picture, REQUESTS_LIMIT, Role,
    StopReason, ToolCall, ToolDefinition, ToolResult, Turn, Unfit, Usage,
};

/// The kind and version of a script.
pub const SCRIPT: &str = "judge-script/1";
/// The kind and version of an exchange.
pub const EXCHANGE: &str = "judge-exchange/1";
/// The most bytes of a script that is read.
pub const SCRIPT_MAX_BYTES: usize = 16 << 20;
/// The most entries of a script: as many requests as one run sends.
pub const SCRIPT_ENTRIES_LIMIT: usize = REQUESTS_LIMIT;
/// The most bytes of an exchange that is read: a request holds at most 64 MiB of text,
/// and writing it as JSON can double that.
pub const EXCHANGE_MAX_BYTES: usize = 160 << 20;

/// How an opaque part's body is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keep {
    /// With its body, as it is.
    Whole,
    /// As the digest of its body and its length, and no body.
    Digests,
}

/// Why bytes were not read as a script or an exchange.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unread {
    /// More bytes than are read. They were not parsed.
    TooLarge { bytes: u64, limit: u64 },
    /// Another kind of document, or another version. Its first 64 characters are kept.
    OtherVersion { found: String },
    /// Not the document: not JSON, a field that is not known, a field left out, or
    /// something after it. The reason is the parser's, and can quote what was read:
    /// whoever prints it escapes it.
    Malformed(String),
    /// More entries than a script holds.
    TooMany { entries: u64, limit: u64 },
    /// A value that the type it is read into refuses: the type's own refusal, and
    /// which entry or message it was met in, from 0.
    Unfit { at: usize, why: Unfit },
    /// A word that names nothing: a fault's kind.
    Unknown { what: &'static str },
    /// An opaque part that was written without its body, or a turn that was not
    /// written: it cannot be made again.
    Withheld,
    /// An image whose pixels the caller does not have.
    NoPicture { pixels: PixelDigest },
}

impl std::fmt::Display for Unread {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unread::TooLarge { bytes, limit } => {
                write!(f, "{bytes} bytes, and at most {limit} are read")
            }
            Unread::OtherVersion { found } => {
                write!(f, "another kind or version: {}", found.escape_debug())
            }
            Unread::Malformed(why) => {
                let shown: String = why.chars().take(200).collect();
                write!(f, "not the document: {}", shown.escape_debug())
            }
            Unread::TooMany { entries, limit } => {
                write!(f, "{entries} entries, and a script holds at most {limit}")
            }
            Unread::Unfit { at, why } => write!(f, "at {at}: {why}"),
            Unread::Unknown { what } => write!(f, "{what} is not one there is"),
            Unread::Withheld => f.write_str(
                "an opaque part was written without its body, or a turn was not written, \
                 and cannot be read",
            ),
            Unread::NoPicture { pixels } => write!(f, "no picture was given for {pixels}"),
        }
    }
}

impl std::error::Error for Unread {}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ImageWire {
    width: u32,
    height: u32,
    pixels: PixelDigest,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum PartWire {
    Text(String),
    Image(ImageWire),
    ToolCall {
        id: String,
        name: String,
        arguments: Value,
    },
    ToolResult {
        call: String,
        text: String,
        error: bool,
        images: Vec<ImageWire>,
    },
    Opaque {
        adapter: String,
        body: String,
    },
    OpaqueWithheld {
        adapter: String,
        digest: Digest,
        bytes: u64,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RoleWire {
    User,
    Assistant,
    System,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MessageWire {
    role: RoleWire,
    parts: Vec<PartWire>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StopWire {
    EndTurn,
    ToolUse,
    MaxTokens,
    Refusal,
    Other(String),
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct UsageWire {
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
}

/// A field that may be null and may not be left out.
fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(deserializer)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TurnWire {
    parts: Vec<PartWire>,
    stop: StopWire,
    #[serde(deserialize_with = "present")]
    usage: Option<UsageWire>,
    #[serde(deserialize_with = "present")]
    request_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FaultWire {
    kind: String,
    sentence: String,
    #[serde(deserialize_with = "present")]
    retry_after_ms: Option<u64>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AnswerWire {
    Turn(TurnWire),
    Fault(FaultWire),
    /// A turn that is not written: how many parts it had, how many bytes of text, the
    /// digest of that text, and why it stopped.
    TurnWithheld {
        parts: u64,
        bytes: u64,
        digest: Digest,
        stop: StopWire,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScriptWire {
    script: String,
    entries: Vec<AnswerWire>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolWire {
    name: String,
    description: String,
    schema: Value,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExchangeWire {
    exchange: String,
    place: u64,
    system: String,
    messages: Vec<MessageWire>,
    tools: Vec<ToolWire>,
    max_output_tokens: u32,
    answer: AnswerWire,
}

fn image_out(picture: &Picture) -> ImageWire {
    ImageWire {
        width: picture.width(),
        height: picture.height(),
        pixels: picture.digest(),
    }
}

fn parts_out(parts: &[Part], opaque: Keep) -> Vec<PartWire> {
    let part = |part: &Part| match part {
        Part::Text(text) => PartWire::Text(text.clone()),
        Part::Image(picture) => PartWire::Image(image_out(picture)),
        Part::ToolCall(call) => PartWire::ToolCall {
            id: call.id().to_owned(),
            name: call.name().to_owned(),
            arguments: call.arguments().clone(),
        },
        Part::ToolResult(result) => PartWire::ToolResult {
            call: result.call().to_owned(),
            text: result.text().to_owned(),
            error: result.is_error(),
            images: result.images().iter().map(image_out).collect(),
        },
        Part::Opaque(part) => match opaque {
            Keep::Whole => PartWire::Opaque {
                adapter: part.adapter().to_owned(),
                body: part.body().to_owned(),
            },
            Keep::Digests => PartWire::OpaqueWithheld {
                adapter: part.adapter().to_owned(),
                digest: Digest::of_bytes(part.body().as_bytes()),
                bytes: part.body().len() as u64,
            },
        },
    };
    parts.iter().map(part).collect()
}

fn stop_out(turn: &Turn) -> StopWire {
    match turn.stop() {
        StopReason::EndTurn => StopWire::EndTurn,
        StopReason::ToolUse => StopWire::ToolUse,
        StopReason::MaxTokens => StopWire::MaxTokens,
        StopReason::Refusal => StopWire::Refusal,
        StopReason::Other(word) => StopWire::Other(word.clone()),
    }
}

/// A turn as an exchange holds it when the turn itself is not written: its sizes, the
/// SHA-256 digest of its text (each text, each opaque body and each call's arguments
/// as written, in order, with nothing between them), and why it stopped.
fn withheld_out(turn: &Turn) -> AnswerWire {
    use sha2::{Digest as _, Sha256};
    let mut hash = Sha256::new();
    for part in turn.parts() {
        match part {
            Part::Text(text) => hash.update(text.as_bytes()),
            Part::Opaque(part) => hash.update(part.body().as_bytes()),
            Part::ToolCall(call) => hash.update(call.arguments().to_string().as_bytes()),
            _ => {}
        }
    }
    AnswerWire::TurnWithheld {
        parts: turn.parts().len() as u64,
        bytes: turn.text_bytes(),
        digest: Digest::from_bytes(hash.finalize().into()),
        stop: stop_out(turn),
    }
}

fn answer_out(answer: &Result<Turn, Fault>, opaque: Keep) -> AnswerWire {
    match answer {
        Ok(turn) => AnswerWire::Turn(TurnWire {
            parts: parts_out(turn.parts(), opaque),
            stop: stop_out(turn),
            usage: turn.usage().map(|usage| UsageWire {
                input_tokens: usage.input_tokens(),
                output_tokens: usage.output_tokens(),
                cache_read_tokens: usage.cache_read_tokens(),
                cache_write_tokens: usage.cache_write_tokens(),
            }),
            request_id: turn.request_id().map(str::to_owned),
        }),
        Err(fault) => AnswerWire::Fault(FaultWire {
            kind: fault.kind().name().to_owned(),
            sentence: fault.sentence().to_owned(),
            retry_after_ms: fault.wait_ms(),
        }),
    }
}

/// JSON, then a newline. What is written holds strings, whole numbers and JSON values
/// that were read as JSON: writing it cannot fail.
fn bytes_of(document: &impl Serialize) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(document).unwrap_or_default();
    bytes.push(b'\n');
    bytes
}

/// A script of these answers, in order, with every opaque part whole.
pub fn write_script(entries: &[Result<Turn, Fault>]) -> Vec<u8> {
    bytes_of(&ScriptWire {
        script: SCRIPT.to_owned(),
        entries: entries
            .iter()
            .map(|answer| answer_out(answer, Keep::Whole))
            .collect(),
    })
}

/// The exchange that was the request at `place` among its run's requests, from 0: what
/// was sent, whole but for pixels, and what came back.
pub fn write_exchange(
    place: usize,
    request: &JudgeRequest,
    answer: &Result<Turn, Fault>,
    opaque: Keep,
) -> Vec<u8> {
    head(place, request, opaque).with(&answer_out(answer, opaque))
}

/// An exchange written as far as its answer: everything that was sent. A handle writes
/// it before it sends, so that it knows what the exchange will take, and adds the
/// answer when it has one.
pub(crate) struct Head(String);

impl Head {
    /// How many bytes the exchange takes before its answer.
    pub(crate) fn len(&self) -> u64 {
        self.0.len() as u64
    }

    fn with(&self, answer: &AnswerWire) -> Vec<u8> {
        let answer = serde_json::to_string(answer).unwrap_or_default();
        format!("{},\"answer\":{answer}}}\n", self.0).into_bytes()
    }

    /// The exchange, with this answer, as a run stores it.
    pub(crate) fn answered(&self, answer: &Result<Turn, Fault>) -> Vec<u8> {
        self.with(&answer_out(answer, Keep::Digests))
    }

    /// The exchange, with this turn as its answer and the turn itself not written.
    pub(crate) fn unwritten(&self, turn: &Turn) -> Vec<u8> {
        self.with(&withheld_out(turn))
    }
}

#[derive(Serialize)]
struct HeadWire {
    exchange: String,
    place: u64,
    system: String,
    messages: Vec<MessageWire>,
    tools: Vec<ToolWire>,
    max_output_tokens: u32,
}

/// What was sent, written: the exchange up to its answer.
pub(crate) fn head(place: usize, request: &JudgeRequest, opaque: Keep) -> Head {
    let messages = request.messages().iter().map(|message| MessageWire {
        role: match message.role() {
            Role::User => RoleWire::User,
            Role::Assistant => RoleWire::Assistant,
            Role::System => RoleWire::System,
        },
        parts: parts_out(message.parts(), opaque),
    });
    let tools = request.tools().iter().map(|tool| ToolWire {
        name: tool.name().to_owned(),
        description: tool.description().to_owned(),
        schema: tool.schema().clone(),
    });
    let mut written = serde_json::to_string(&HeadWire {
        exchange: EXCHANGE.to_owned(),
        place: place as u64,
        system: request.system().to_owned(),
        messages: messages.collect(),
        tools: tools.collect(),
        max_output_tokens: request.controls().max_output_tokens(),
    })
    .unwrap_or_default();
    // The object is left open: the answer is its last field.
    written.pop();
    Head(written)
}

/// The document, when the bytes are within `limit` and are of this kind and version.
fn document<'de, T: Deserialize<'de>>(
    bytes: &'de [u8],
    limit: usize,
    field: &str,
    kind: &str,
) -> Result<T, Unread> {
    if bytes.len() > limit {
        return Err(Unread::TooLarge {
            bytes: bytes.len() as u64,
            limit: limit as u64,
        });
    }
    let malformed = |error: serde_json::Error| Unread::Malformed(error.to_string());
    // The kind is looked at first, by itself: another version has another shape.
    // Nothing else of the document is kept by this look: every other field is passed
    // over as it is parsed.
    let found = peek(bytes, field).map_err(malformed)?;
    if found.as_deref() != Some(kind) {
        let found = found.as_deref().unwrap_or("none");
        return Err(Unread::OtherVersion {
            found: found.chars().take(64).collect(),
        });
    }
    let mut reader = serde_json::Deserializer::from_slice(bytes);
    let document = T::deserialize(&mut reader).map_err(malformed)?;
    reader.end().map_err(malformed)?;
    Ok(document)
}

/// The most bytes of the text that says a document's kind and version.
pub const KIND_MAX_BYTES: usize = 64;

/// The text under `field` at the top of the document, when there is one: none when
/// the field is not there, or holds something that is no text, or a text of more than
/// [`KIND_MAX_BYTES`], which is not kept.
fn peek(bytes: &[u8], field: &str) -> Result<Option<String>, serde_json::Error> {
    use serde::de::{IgnoredAny, MapAccess, Visitor};

    /// The value under the kind's field: a text of at most [`KIND_MAX_BYTES`], kept, or
    /// anything else, a longer text included, passed over and not kept.
    struct Short(Option<String>);

    impl<'de> Deserialize<'de> for Short {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Short, D::Error> {
            struct Value;

            impl<'de> Visitor<'de> for Value {
                type Value = Short;

                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("any JSON value")
                }

                fn visit_str<E>(self, text: &str) -> Result<Short, E> {
                    let short = text.len() <= KIND_MAX_BYTES;
                    Ok(Short(short.then(|| text.to_owned())))
                }

                fn visit_bool<E>(self, _: bool) -> Result<Short, E> {
                    Ok(Short(None))
                }

                fn visit_i64<E>(self, _: i64) -> Result<Short, E> {
                    Ok(Short(None))
                }

                fn visit_u64<E>(self, _: u64) -> Result<Short, E> {
                    Ok(Short(None))
                }

                fn visit_f64<E>(self, _: f64) -> Result<Short, E> {
                    Ok(Short(None))
                }

                fn visit_unit<E>(self) -> Result<Short, E> {
                    Ok(Short(None))
                }

                fn visit_seq<A: serde::de::SeqAccess<'de>>(
                    self,
                    mut seq: A,
                ) -> Result<Short, A::Error> {
                    while seq.next_element::<IgnoredAny>()?.is_some() {}
                    Ok(Short(None))
                }

                fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Short, A::Error> {
                    while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
                    Ok(Short(None))
                }
            }

            deserializer.deserialize_any(Value)
        }
    }

    struct Top<'a>(&'a str);

    impl<'de> Visitor<'de> for Top<'_> {
        type Value = Option<String>;

        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a JSON object")
        }

        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Option<String>, A::Error> {
            let mut found = None;
            while let Some(name) = map.next_key::<std::borrow::Cow<'de, str>>()? {
                if name == self.0 {
                    found = map.next_value::<Short>()?.0;
                } else {
                    map.next_value::<IgnoredAny>()?;
                }
            }
            Ok(found)
        }
    }

    let mut reader = serde_json::Deserializer::from_slice(bytes);
    let found = serde::Deserializer::deserialize_map(&mut reader, Top(field))?;
    reader.end()?;
    Ok(found)
}

/// Where a picture comes from when a document names one.
type Pictures<'a> = &'a dyn Fn(PixelDigest) -> Option<Picture>;

fn image_in(image: ImageWire, pictures: Pictures<'_>) -> Result<Picture, Unread> {
    let picture = pictures(image.pixels).ok_or(Unread::NoPicture {
        pixels: image.pixels,
    })?;
    if (picture.width(), picture.height()) != (image.width, image.height) {
        return Err(Unread::NoPicture {
            pixels: image.pixels,
        });
    }
    Ok(picture)
}

fn parts_in(parts: Vec<PartWire>, at: usize, pictures: Pictures<'_>) -> Result<Vec<Part>, Unread> {
    let unfit = |why: Unfit| Unread::Unfit { at, why };
    let part = |part: PartWire| {
        Ok(match part {
            PartWire::Text(text) => Part::Text(text),
            PartWire::Image(image) => Part::Image(image_in(image, pictures)?),
            PartWire::ToolCall {
                id,
                name,
                arguments,
            } => Part::ToolCall(ToolCall::new(&id, &name, arguments).map_err(unfit)?),
            PartWire::ToolResult {
                call,
                text,
                error,
                images,
            } => {
                let images = images.into_iter().map(|image| image_in(image, pictures));
                let images = images.collect::<Result<_, _>>()?;
                Part::ToolResult(ToolResult::new(&call, &text, error, images).map_err(unfit)?)
            }
            PartWire::Opaque { adapter, body } => {
                Part::Opaque(Opaque::new(&adapter, &body).map_err(unfit)?)
            }
            PartWire::OpaqueWithheld { .. } => return Err(Unread::Withheld),
        })
    };
    parts.into_iter().map(part).collect()
}

fn answer_in(answer: AnswerWire, at: usize) -> Result<Result<Turn, Fault>, Unread> {
    Ok(match answer {
        AnswerWire::Turn(turn) => {
            // A turn holds no image: one in the data is a part only a user says, which
            // the turn's constructor refuses, so no picture is ever asked for here.
            let none = |_| None;
            let parts = parts_in(turn.parts, at, &none);
            let parts = match parts {
                Err(Unread::NoPicture { .. }) => {
                    return Err(Unread::Unfit {
                        at,
                        why: Unfit::NotAllowed {
                            what: "a turn",
                            why: "holds what only a user says",
                        },
                    });
                }
                parts => parts?,
            };
            let stop = match turn.stop {
                StopWire::EndTurn => StopReason::EndTurn,
                StopWire::ToolUse => StopReason::ToolUse,
                StopWire::MaxTokens => StopReason::MaxTokens,
                StopWire::Refusal => StopReason::Refusal,
                StopWire::Other(word) => StopReason::Other(word),
            };
            let usage = turn.usage.map(|usage| {
                Usage::new(usage.input_tokens, usage.output_tokens)
                    .cached(usage.cache_read_tokens, usage.cache_write_tokens)
            });
            let turn = Turn::new(parts, stop, usage, turn.request_id.as_deref());
            Ok(turn.map_err(|why| Unread::Unfit { at, why })?)
        }
        AnswerWire::TurnWithheld { .. } => return Err(Unread::Withheld),
        AnswerWire::Fault(fault) => {
            let kind = FaultKind::ALL
                .into_iter()
                .find(|kind| kind.name() == fault.kind);
            let kind = kind.ok_or(Unread::Unknown {
                what: "a fault's kind",
            })?;
            let made = Fault::new(kind, &fault.sentence);
            Err(match fault.retry_after_ms {
                Some(wait) => made.retry_after_ms(wait),
                None => made,
            })
        }
    })
}

/// Reads a script: the answers a scripted judge plays, in order. Each turn is made by
/// [`Turn::new`] and each fault by [`Fault::new`], so a script holds nothing those
/// refuse or would have cleaned.
pub fn read_script(bytes: &[u8]) -> Result<Vec<Result<Turn, Fault>>, Unread> {
    let script: ScriptWire = document(bytes, SCRIPT_MAX_BYTES, "script", SCRIPT)?;
    if script.entries.len() > SCRIPT_ENTRIES_LIMIT {
        return Err(Unread::TooMany {
            entries: script.entries.len() as u64,
            limit: SCRIPT_ENTRIES_LIMIT as u64,
        });
    }
    let entries = script.entries.into_iter().enumerate();
    entries.map(|(at, answer)| answer_in(answer, at)).collect()
}

/// An exchange that was read back.
#[derive(Debug, Clone, PartialEq)]
pub struct Exchange {
    /// Its place among its run's requests, from 0.
    pub place: u64,
    pub request: JudgeRequest,
    pub answer: Result<Turn, Fault>,
}

/// Reads an exchange that was written with every opaque part whole. `pictures` gives
/// the picture of a digest the exchange names. The request is made by
/// [`JudgeRequest::new`] and everything in it by its own constructor; a message is
/// placed by its number among the messages, and the answer after them.
pub fn read_exchange(
    bytes: &[u8],
    pictures: &dyn Fn(PixelDigest) -> Option<Picture>,
) -> Result<Exchange, Unread> {
    let read: ExchangeWire = document(bytes, EXCHANGE_MAX_BYTES, "exchange", EXCHANGE)?;
    let last = read.messages.len();
    let mut messages = Vec::new();
    for (at, message) in read.messages.into_iter().enumerate() {
        let role = match message.role {
            RoleWire::User => Role::User,
            RoleWire::Assistant => Role::Assistant,
            RoleWire::System => Role::System,
        };
        let parts = parts_in(message.parts, at, pictures)?;
        messages.push(Message::new(role, parts).map_err(|why| Unread::Unfit { at, why })?);
    }
    let unfit = |why: Unfit| Unread::Unfit { at: last, why };
    let tools = read
        .tools
        .into_iter()
        .map(|tool| ToolDefinition::new(&tool.name, &tool.description, tool.schema).map_err(unfit));
    let tools = tools.collect::<Result<_, _>>()?;
    let controls = Controls::new(read.max_output_tokens).map_err(unfit)?;
    let request = JudgeRequest::new(&read.system, messages, tools, controls).map_err(unfit)?;
    Ok(Exchange {
        place: read.place,
        request,
        answer: answer_in(read.answer, last)?,
    })
}
