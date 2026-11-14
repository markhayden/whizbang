//! Judge (`SPEC.md` section 4; ADR 0006, "Judge"): a conversation with images, tool
//! definitions and provider-opaque content goes out with its controls; a turn comes
//! back, with its usage, or a fault.
//!
//! Content is ordered and mixed: a message, and a turn, is a list of parts, and the
//! order of the list is the order the model wrote or will read them in. A part that
//! only its adapter reads (thinking, a signature, a reasoning item) is an [`Opaque`]:
//! whizbang holds its bytes as they came and gives them back as they are.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::contract::{Cancellation, Capability, text};

use super::fault::{Raw, clean};
use super::{Bounds, Declaration, Fault, FaultKind, Picture, Unfit, feature};

/// The most messages a conversation holds.
pub const MESSAGE_LIMIT: usize = 1024;
/// The most parts a message or a turn holds.
pub const PART_LIMIT: usize = 256;
/// The most bytes of one text: a text part, a system prompt, a tool result's text, an
/// opaque part's body, and a tool call's arguments or a tool's schema written as JSON.
pub const TEXT_LIMIT: usize = 1 << 20;
/// The most bytes of text a conversation holds in all, the system prompt included.
pub const CONVERSATION_LIMIT: u64 = 64 << 20;
/// The most tools a request defines.
pub const TOOL_LIMIT: usize = 128;
/// The most images a tool result holds.
pub const RESULT_IMAGE_LIMIT: usize = 16;
/// The most bytes of a tool's description.
pub const DESCRIPTION_LIMIT: usize = 4096;

const ID_LIMIT: usize = 128;
const NAME_LIMIT: usize = 64;

fn too_large(what: &'static str, size: usize, limit: usize) -> Unfit {
    Unfit::TooLarge {
        what,
        size: size as u64,
        limit: limit as u64,
    }
}

/// A text of 1 to [`TEXT_LIMIT`] bytes.
fn check_text(text: &str) -> Result<(), Unfit> {
    if text.is_empty() {
        return Err(Unfit::Empty { what: "a text" });
    }
    if text.len() > TEXT_LIMIT {
        return Err(too_large("a text, in bytes,", text.len(), TEXT_LIMIT));
    }
    Ok(())
}

/// Whether the text is 1 to `limit` printable ASCII characters with no space.
fn is_plain(text: &str, limit: usize) -> bool {
    !text.is_empty() && text.len() <= limit && text.bytes().all(|b| b.is_ascii_graphic())
}

/// How many bytes a JSON value takes when it is written.
fn json_size(value: &Value) -> usize {
    serde_json::to_string(value).map_or(usize::MAX, |written| written.len())
}

/// Who speaks a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    /// The harness, for the person or the tool: text, images and tool results.
    User,
    /// The model: text, tool calls and opaque parts.
    Assistant,
    /// An instruction of the harness inside the conversation: text only.
    System,
}

/// A part that only the adapter that made it reads: thinking, a signature, a reasoning
/// item. Its body is the bytes the provider sent, as text: UTF-8, of 1 to [`TEXT_LIMIT`]
/// bytes. A part over the limit is refused, never cut: an adapter that is sent one
/// fails the turn with the fault `too_large`. Whizbang never parses,
/// orders, trims or prints it again; it gives it back as it is. What `{:?}` prints of
/// one is its adapter and its length.
#[derive(Clone, PartialEq, Eq)]
pub struct Opaque {
    adapter: String,
    body: String,
}

impl Opaque {
    /// A part of this adapter, which is named as a backend is, with a body of 1 to
    /// [`TEXT_LIMIT`] bytes.
    pub fn new(adapter: &str, body: &str) -> Result<Opaque, Unfit> {
        let what = "an opaque part";
        if text::action_name(adapter).is_err() {
            return Err(Unfit::NotAllowed {
                what,
                why: "names an adapter that is not a name",
            });
        }
        if body.is_empty() {
            return Err(Unfit::Empty { what });
        }
        if body.len() > TEXT_LIMIT {
            return Err(too_large(
                "an opaque part, in bytes,",
                body.len(),
                TEXT_LIMIT,
            ));
        }
        Ok(Opaque {
            adapter: adapter.to_owned(),
            body: body.to_owned(),
        })
    }

    /// The one adapter that reads the part.
    pub fn adapter(&self) -> &str {
        &self.adapter
    }

    /// The part, as it came.
    pub fn body(&self) -> &str {
        &self.body
    }
}

impl std::fmt::Debug for Opaque {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Opaque({}, {} bytes)", self.adapter, self.body.len())
    }
}

/// A model's call of a tool. What `{:?}` prints of one is its id, the name it calls and
/// how long its arguments are.
#[derive(Clone, PartialEq)]
pub struct ToolCall {
    id: String,
    name: String,
    arguments: Value,
    size: usize,
}

impl ToolCall {
    /// A call. Its id is 1 to 128 printable ASCII characters with no space, and its
    /// name 1 to 64: the name is what the model wrote, and need not be a tool that
    /// exists. Its arguments are any JSON value of at most [`TEXT_LIMIT`] bytes when
    /// written.
    pub fn new(id: &str, name: &str, arguments: Value) -> Result<ToolCall, Unfit> {
        let what = "a tool call";
        if !is_plain(id, ID_LIMIT) {
            return Err(Unfit::NotAllowed {
                what,
                why: "has an id that is not 1 to 128 printable ASCII characters with no space",
            });
        }
        if !is_plain(name, NAME_LIMIT) {
            return Err(Unfit::NotAllowed {
                what,
                why: "names a tool in other than 1 to 64 printable ASCII characters with no space",
            });
        }
        let size = json_size(&arguments);
        if size > TEXT_LIMIT {
            return Err(too_large(
                "the arguments of a tool call, in bytes,",
                size,
                TEXT_LIMIT,
            ));
        }
        Ok(ToolCall {
            id: id.to_owned(),
            name: name.to_owned(),
            arguments,
            size,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn arguments(&self) -> &Value {
        &self.arguments
    }
}

impl std::fmt::Debug for ToolCall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ToolCall({}, {}, arguments of {} bytes)",
            self.id, self.name, self.size
        )
    }
}

/// What a tool answered a call with: text, images, or both, and whether it is an error.
/// What `{:?}` prints of one is the call it answers, how long its text is, and its
/// pictures by size and digest.
#[derive(Clone, PartialEq)]
pub struct ToolResult {
    call: String,
    text: String,
    error: bool,
    images: Vec<Picture>,
}

impl ToolResult {
    /// The answer to the call of this id: a text of at most [`TEXT_LIMIT`] bytes, at
    /// most [`RESULT_IMAGE_LIMIT`] images, and something in one of them.
    pub fn new(
        call: &str,
        text: &str,
        error: bool,
        images: Vec<Picture>,
    ) -> Result<ToolResult, Unfit> {
        let what = "a tool result";
        if !is_plain(call, ID_LIMIT) {
            return Err(Unfit::NotAllowed {
                what,
                why: "answers an id that is not 1 to 128 printable ASCII characters with no space",
            });
        }
        if text.is_empty() && images.is_empty() {
            return Err(Unfit::Empty { what });
        }
        if text.len() > TEXT_LIMIT {
            return Err(too_large("a text, in bytes,", text.len(), TEXT_LIMIT));
        }
        if images.len() > RESULT_IMAGE_LIMIT {
            return Err(too_large(
                "a tool result, in images,",
                images.len(),
                RESULT_IMAGE_LIMIT,
            ));
        }
        Ok(ToolResult {
            call: call.to_owned(),
            text: text.to_owned(),
            error,
            images,
        })
    }

    /// The id of the call this answers.
    pub fn call(&self) -> &str {
        &self.call
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Whether the tool failed, or refused the call.
    pub fn is_error(&self) -> bool {
        self.error
    }

    pub fn images(&self) -> &[Picture] {
        &self.images
    }
}

impl std::fmt::Debug for ToolResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ToolResult({}, a text of {} bytes, error {}, {:?})",
            self.call,
            self.text.len(),
            self.error,
            self.images
        )
    }
}

/// One piece of a message or a turn. A part on its own is checked by nothing: a
/// [`Message`] and a [`Turn`] check every part they are made of. What `{:?}` prints of
/// a part is its kind and its size, never a text, a body or a pixel: a conversation is
/// a prompt and a model's answers, and megabytes of them.
#[derive(Clone, PartialEq)]
#[non_exhaustive]
pub enum Part {
    Text(String),
    Image(Picture),
    ToolCall(ToolCall),
    ToolResult(ToolResult),
    Opaque(Opaque),
}

impl std::fmt::Debug for Part {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Part::Text(text) => write!(f, "Text({} bytes)", text.len()),
            Part::Image(picture) => picture.fmt(f),
            Part::ToolCall(call) => call.fmt(f),
            Part::ToolResult(result) => result.fmt(f),
            Part::Opaque(opaque) => opaque.fmt(f),
        }
    }
}

impl Part {
    /// The bytes of text the part holds.
    fn bytes(&self) -> u64 {
        (match self {
            Part::Text(text) => text.len(),
            Part::Image(_) => 0,
            Part::ToolCall(call) => call.size,
            Part::ToolResult(result) => result.text.len(),
            Part::Opaque(opaque) => opaque.body.len(),
        }) as u64
    }
}

/// One message of a conversation: who speaks, and parts in order.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    role: Role,
    parts: Vec<Part>,
    bytes: u64,
}

impl Message {
    /// A message of 1 to [`PART_LIMIT`] parts, each of which its role can say: a system
    /// message holds text only; a user message text, images and tool results; an
    /// assistant message text, tool calls and opaque parts. A text is 1 to
    /// [`TEXT_LIMIT`] bytes.
    pub fn new(role: Role, parts: Vec<Part>) -> Result<Message, Unfit> {
        if parts.is_empty() {
            return Err(Unfit::Empty { what: "a message" });
        }
        if parts.len() > PART_LIMIT {
            return Err(too_large("a message, in parts,", parts.len(), PART_LIMIT));
        }
        for part in &parts {
            let allowed = match (role, part) {
                (_, Part::Text(text)) => {
                    check_text(text)?;
                    true
                }
                (Role::User, Part::Image(_) | Part::ToolResult(_)) => true,
                (Role::Assistant, Part::ToolCall(_) | Part::Opaque(_)) => true,
                _ => false,
            };
            if !allowed {
                return Err(match role {
                    Role::System => Unfit::NotAllowed {
                        what: "a system message",
                        why: "holds text only",
                    },
                    Role::User => Unfit::NotAllowed {
                        what: "a user message",
                        why: "holds text, images and tool results only",
                    },
                    Role::Assistant => Unfit::NotAllowed {
                        what: "an assistant message",
                        why: "holds text, tool calls and opaque parts only",
                    },
                });
            }
        }
        let bytes = parts.iter().map(Part::bytes).sum();
        Ok(Message { role, parts, bytes })
    }

    pub fn role(&self) -> Role {
        self.role
    }

    /// The parts, in order.
    pub fn parts(&self) -> &[Part] {
        &self.parts
    }
}

/// A tool as the model is told of it: a name, a description and the schema of its
/// arguments.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolDefinition {
    name: String,
    description: String,
    schema: Value,
    /// What the tool counts toward a conversation's bound: its name, its description
    /// and its schema as written.
    bytes: u64,
}

impl ToolDefinition {
    /// A tool. Its name keeps the rule of an action's name; its description is 1 to
    /// [`DESCRIPTION_LIMIT`] bytes; its schema is a JSON object of at most
    /// [`TEXT_LIMIT`] bytes when written.
    pub fn new(name: &str, description: &str, schema: Value) -> Result<ToolDefinition, Unfit> {
        if text::action_name(name).is_err() {
            return Err(Unfit::NotAllowed {
                what: "a tool",
                why: "has a name that is not a name",
            });
        }
        if description.is_empty() {
            return Err(Unfit::Empty {
                what: "a tool's description",
            });
        }
        if description.len() > DESCRIPTION_LIMIT {
            return Err(too_large(
                "a tool's description, in bytes,",
                description.len(),
                DESCRIPTION_LIMIT,
            ));
        }
        if !schema.is_object() {
            return Err(Unfit::NotAllowed {
                what: "a tool's schema",
                why: "is not a JSON object",
            });
        }
        let size = json_size(&schema);
        if size > TEXT_LIMIT {
            return Err(too_large("a tool's schema, in bytes,", size, TEXT_LIMIT));
        }
        Ok(ToolDefinition {
            name: name.to_owned(),
            description: description.to_owned(),
            schema,
            bytes: (name.len() + description.len() + size) as u64,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn schema(&self) -> &Value {
        &self.schema
    }
}

/// The controls of a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Controls {
    max_output_tokens: u32,
}

impl Controls {
    /// Controls that allow the model at most this many output tokens, and at least 1.
    pub fn new(max_output_tokens: u32) -> Result<Controls, Unfit> {
        if max_output_tokens == 0 {
            return Err(Unfit::OutOfRange {
                what: "the most output tokens",
                at: None,
                found: "0".to_owned(),
                allowed: "1 or more",
            });
        }
        Ok(Controls { max_output_tokens })
    }

    pub fn max_output_tokens(&self) -> u32 {
        self.max_output_tokens
    }
}

/// What a judge is sent: the system prompt, the conversation so far, the tools on
/// offer, and the controls. What `{:?}` prints of one is the system prompt's length,
/// each message by its parts' kinds and sizes, the tools' names and the controls.
#[derive(Clone, PartialEq)]
pub struct JudgeRequest {
    system: String,
    messages: Vec<Message>,
    tools: Vec<ToolDefinition>,
    controls: Controls,
}

impl std::fmt::Debug for JudgeRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let tools: Vec<&str> = self.tools.iter().map(|tool| tool.name.as_str()).collect();
        f.debug_struct("JudgeRequest")
            .field("system_bytes", &self.system.len())
            .field("messages", &self.messages)
            .field("tools", &tools)
            .field("controls", &self.controls)
            .finish()
    }
}

impl JudgeRequest {
    /// A request. The system prompt is at most [`TEXT_LIMIT`] bytes and may be empty;
    /// the conversation is 1 to [`MESSAGE_LIMIT`] messages; there are at most
    /// [`TOOL_LIMIT`] tools, no two of one name; and the request holds at most
    /// [`CONVERSATION_LIMIT`] bytes of text in all, counting the system prompt, every
    /// message, and each tool's name, description and schema as written.
    ///
    /// The shape of the conversation is not checked: the order of roles, whether a
    /// result answers a call, two calls of one id in a message. That is the harness's
    /// to keep and the provider's to refuse.
    pub fn new(
        system: &str,
        messages: Vec<Message>,
        tools: Vec<ToolDefinition>,
        controls: Controls,
    ) -> Result<JudgeRequest, Unfit> {
        if system.len() > TEXT_LIMIT {
            return Err(too_large("a text, in bytes,", system.len(), TEXT_LIMIT));
        }
        let what = "the conversation";
        if messages.is_empty() {
            return Err(Unfit::Empty { what });
        }
        if messages.len() > MESSAGE_LIMIT {
            return Err(too_large(
                "the conversation, in messages,",
                messages.len(),
                MESSAGE_LIMIT,
            ));
        }
        let bytes = system.len() as u64
            + messages.iter().map(|m| m.bytes).sum::<u64>()
            + tools.iter().map(|tool| tool.bytes).sum::<u64>();
        if bytes > CONVERSATION_LIMIT {
            return Err(Unfit::TooLarge {
                what: "the conversation, in bytes of text,",
                size: bytes,
                limit: CONVERSATION_LIMIT,
            });
        }
        if tools.len() > TOOL_LIMIT {
            return Err(too_large("the tools", tools.len(), TOOL_LIMIT));
        }
        let mut names = BTreeSet::new();
        if !tools.iter().all(|tool| names.insert(tool.name.as_str())) {
            return Err(Unfit::NotAllowed {
                what: "the tools",
                why: "define one name twice",
            });
        }
        Ok(JudgeRequest {
            system: system.to_owned(),
            messages,
            tools,
            controls,
        })
    }

    pub fn system(&self) -> &str {
        &self.system
    }

    /// The conversation so far, in order.
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    pub fn tools(&self) -> &[ToolDefinition] {
        &self.tools
    }

    pub fn controls(&self) -> Controls {
        self.controls
    }

    /// Every picture in the conversation: in a message, and in a tool result.
    fn has_image(&self) -> bool {
        self.messages
            .iter()
            .flat_map(|message| &message.parts)
            .any(|part| match part {
                Part::Image(_) => true,
                Part::ToolResult(result) => !result.images.is_empty(),
                _ => false,
            })
    }
}

/// Why a turn ended.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum StopReason {
    /// The model finished what it had to say.
    EndTurn,
    /// The model stopped to have its tool calls answered.
    ToolUse,
    /// The turn was cut off at the most output tokens.
    MaxTokens,
    /// The model declined.
    Refusal,
    /// Another reason, in the provider's word. A turn keeps its first 64 characters,
    /// cleaned as a fault's sentence is.
    Other(String),
}

/// The tokens a turn used, as the backend reported them. It is made through
/// [`Usage::new`] and can gain a count without breaking whoever makes or reads one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct Usage {
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
}

impl Usage {
    /// The input tokens that were not read from the provider's cache, and the output
    /// tokens.
    pub fn new(input_tokens: u64, output_tokens: u64) -> Usage {
        Usage {
            input_tokens,
            output_tokens,
            ..Usage::default()
        }
    }

    /// The same usage, with the input tokens read from the provider's prompt cache and
    /// those written to it.
    pub fn cached(mut self, read_tokens: u64, write_tokens: u64) -> Usage {
        self.cache_read_tokens = read_tokens;
        self.cache_write_tokens = write_tokens;
        self
    }

    /// Input tokens that were not read from the provider's cache.
    pub fn input_tokens(&self) -> u64 {
        self.input_tokens
    }

    pub fn output_tokens(&self) -> u64 {
        self.output_tokens
    }

    /// Input tokens read from the provider's prompt cache.
    pub fn cache_read_tokens(&self) -> u64 {
        self.cache_read_tokens
    }

    /// Input tokens written to the provider's prompt cache.
    pub fn cache_write_tokens(&self) -> u64 {
        self.cache_write_tokens
    }
}

/// What a judge answered: ordered content, why it stopped, and what it used. Two turns
/// are equal when those are, and the provider's id.
#[derive(Clone)]
pub struct Turn {
    parts: Vec<Part>,
    stop: StopReason,
    usage: Option<Usage>,
    request_id: Option<String>,
    /// A provider's word for another stop reason, and its id, as they were given:
    /// until a handle has looked for secrets in them.
    raw_stop: Option<Raw>,
    raw_id: Option<Raw>,
}

impl PartialEq for Turn {
    fn eq(&self, other: &Turn) -> bool {
        (&self.parts, &self.stop, &self.usage, &self.request_id)
            == (&other.parts, &other.stop, &other.usage, &other.request_id)
    }
}

impl std::fmt::Debug for Turn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Turn")
            .field("parts", &self.parts)
            .field("stop", &self.stop)
            .field("usage", &self.usage)
            .field("request_id", &self.request_id)
            .finish()
    }
}

/// A provider's id, from its text: trimmed, cleaned and cut.
fn id_of(text: &str) -> String {
    clean(text.trim(), ID_LIMIT)
}

impl Turn {
    /// A turn. Its parts are what an assistant says, at most [`PART_LIMIT`] of them:
    /// text, tool calls and opaque parts, in the order the model wrote them, holding at
    /// most [`CONVERSATION_LIMIT`] bytes of text in all. Refused: a turn with nothing in
    /// it that ended or stopped for a tool; two tool calls with one id; a turn that
    /// stopped for a tool and calls none.
    ///
    /// `usage` is none when the backend reported none, which is not the same as having
    /// used nothing. `request_id` is the provider's name for the exchange; it and
    /// another stop reason are a provider's words, and are kept cleaned and cut, at
    /// 128 and at 64 characters.
    pub fn new(
        parts: Vec<Part>,
        stop: StopReason,
        usage: Option<Usage>,
        request_id: Option<&str>,
    ) -> Result<Turn, Unfit> {
        let what = "a turn";
        let ends = matches!(stop, StopReason::EndTurn | StopReason::ToolUse);
        if parts.is_empty() && ends {
            return Err(Unfit::Empty { what });
        }
        if parts.len() > PART_LIMIT {
            return Err(too_large("a turn, in parts,", parts.len(), PART_LIMIT));
        }
        let mut ids = BTreeSet::new();
        for part in &parts {
            match part {
                Part::Text(text) => check_text(text)?,
                Part::Opaque(_) => {}
                Part::ToolCall(call) => {
                    if !ids.insert(call.id.as_str()) {
                        return Err(Unfit::NotAllowed {
                            what,
                            why: "holds two tool calls with one id",
                        });
                    }
                }
                Part::Image(_) | Part::ToolResult(_) => {
                    return Err(Unfit::NotAllowed {
                        what,
                        why: "holds what only a user says",
                    });
                }
            }
        }
        // A turn is said back as a message, so it holds no more than a conversation may.
        let bytes: u64 = parts.iter().map(Part::bytes).sum();
        if bytes > CONVERSATION_LIMIT {
            return Err(Unfit::TooLarge {
                what: "a turn, in bytes of text,",
                size: bytes,
                limit: CONVERSATION_LIMIT,
            });
        }
        if stop == StopReason::ToolUse && ids.is_empty() {
            return Err(Unfit::NotAllowed {
                what,
                why: "stopped to use a tool and calls none",
            });
        }
        let mut raw_stop = None;
        let stop = match stop {
            StopReason::Other(reason) => {
                raw_stop = Some(Raw::of(&reason));
                StopReason::Other(clean(&reason, NAME_LIMIT))
            }
            named => named,
        };
        let raw_id = request_id.map(Raw::of);
        let request_id = request_id.map(id_of).filter(|id| !id.is_empty());
        Ok(Turn {
            parts,
            stop,
            usage,
            request_id,
            raw_stop,
            raw_id,
        })
    }

    /// The same turn as a run may be told of it and may store it: the provider's word
    /// for its stop reason and the provider's id made again from the words that were
    /// given, with the secrets taken out while they were whole, and only then cleaned
    /// and cut. A handle that knows its run's secrets does this to every turn before
    /// it hands it on. A second time changes nothing. The parts are not touched: they
    /// are never cut, and what is stored of them is redacted where it is stored.
    pub(crate) fn redacted(mut self, redact: &dyn Fn(&str) -> String) -> Turn {
        if let Some(raw) = self.raw_stop.take() {
            let shape = |word: &str| clean(word, NAME_LIMIT);
            self.stop = StopReason::Other(raw.shown(redact, shape, "not kept"));
        }
        if let Some(raw) = self.raw_id.take() {
            let id = raw.shown(redact, id_of, "not kept");
            self.request_id = Some(id).filter(|id| !id.is_empty());
        }
        self
    }

    /// The bytes of text the turn holds: its texts, its opaque parts, and its calls'
    /// arguments as written.
    pub fn text_bytes(&self) -> u64 {
        self.parts.iter().map(Part::bytes).sum()
    }

    /// The content, in the order the model wrote it.
    pub fn parts(&self) -> &[Part] {
        &self.parts
    }

    /// The texts of the turn, in order.
    pub fn texts(&self) -> impl Iterator<Item = &str> {
        self.parts.iter().filter_map(|part| match part {
            Part::Text(text) => Some(text.as_str()),
            _ => None,
        })
    }

    /// The tool calls of the turn, in order.
    pub fn tool_calls(&self) -> impl Iterator<Item = &ToolCall> {
        self.parts.iter().filter_map(|part| match part {
            Part::ToolCall(call) => Some(call),
            _ => None,
        })
    }

    pub fn stop(&self) -> &StopReason {
        &self.stop
    }

    /// What the turn used, when the backend reported it.
    pub fn usage(&self) -> Option<Usage> {
        self.usage
    }

    /// The provider's name for the exchange, when it gave one.
    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }

    /// The turn as the assistant's message of the conversation that goes on: the same
    /// parts, whole and in order. A turn with nothing in it is no message.
    pub fn message(&self) -> Option<Message> {
        Message::new(Role::Assistant, self.parts.clone()).ok()
    }

    fn opaque(&self) -> impl Iterator<Item = &Opaque> {
        opaque_of(&self.parts)
    }
}

fn opaque_of(parts: &[Part]) -> impl Iterator<Item = &Opaque> {
    parts.iter().filter_map(|part| match part {
        Part::Opaque(opaque) => Some(opaque),
        _ => None,
    })
}

/// A backend that judges. It answers one request at a time and returns when it has its
/// answer; it never retries and never changes the conversation. It is expected to stop
/// early when the run is cancelled, and to give up at the run's deadline ([`Bounds`]).
pub trait Judge: Send + Sync {
    /// The model that answers, and the features it supports. The backend's name is
    /// the adapter its opaque parts are marked with.
    fn declaration(&self) -> &Declaration;

    /// Sends one request and returns the model's turn.
    fn send(&self, request: &JudgeRequest, bounds: &Bounds) -> Result<Turn, Fault>;
}

/// The handle through which an action asks a judge. It is bound to one run: it holds
/// that run's cancellation signal and deadline, so an action hands it neither.
#[derive(Clone)]
pub struct JudgeHandle {
    backend: Arc<dyn Judge>,
    bounds: Bounds,
    /// The most bytes of text a turn may hold to be taken, when the action set one.
    turn_bytes: Option<u64>,
}

impl JudgeHandle {
    /// A handle on this judge for the run with this cancellation signal.
    pub fn new(backend: Arc<dyn Judge>, cancellation: Cancellation) -> JudgeHandle {
        JudgeHandle {
            backend,
            bounds: Bounds::new(cancellation),
            turn_bytes: None,
        }
    }

    /// The same handle, for a run that must be over by this time. A deadline is only
    /// ever narrowed: the executor sets it, and an action that holds the handle, or a
    /// clone of it, may shorten it; a later time than the one already set changes
    /// nothing.
    pub fn until(mut self, deadline: Instant) -> JudgeHandle {
        self.bounds.until(deadline);
        self
    }

    /// The same handle, counting each request it sends in this tally, which the
    /// executor waits on before it ends the run (ADR 0008, "Nested calls").
    pub(crate) fn counted(mut self, tally: std::sync::Arc<super::Tally>) -> JudgeHandle {
        self.bounds.counted(tally);
        self
    }

    /// The same handle, holding its deadline to this clock in place of the machine's.
    pub(crate) fn timed_by(mut self, clock: Arc<dyn crate::contract::Clock>) -> JudgeHandle {
        self.bounds.timed_by(clock);
        self
    }

    /// What the judge behind the handle declares.
    pub fn declaration(&self) -> &Declaration {
        self.backend.declaration()
    }

    /// Sends the request and hands back the turn. Nothing is sent once the run is
    /// cancelled or its deadline has passed; nor when the conversation holds an opaque part of another adapter
    /// than this backend; nor when it holds an image, or defines a tool, and the judge
    /// does not declare that feature. A turn that holds another adapter's opaque part
    /// is a malformed answer.
    ///
    /// A handle that an executor's context made keeps the exchange (ADR 0009): the
    /// whole of what was sent and of what came back as a diagnostic of the run, and
    /// what the run's record says of it.
    pub fn send(&self, request: &JudgeRequest) -> Result<Turn, Fault> {
        self.bounds.told(self.asked(request))
    }

    /// The same handle, taking no turn that holds more than this many bytes of text
    /// ([`Turn::text_bytes`]): a larger one is the fault `too_large`, and what the run
    /// stores of it is its sizes and a digest, not the turn. A limit is only ever
    /// narrowed, as a deadline is.
    pub fn turns_within(mut self, bytes: u64) -> JudgeHandle {
        self.turn_bytes = Some(self.turn_bytes.map_or(bytes, |set| set.min(bytes)));
        self
    }

    fn asked(&self, request: &JudgeRequest) -> Result<Turn, Fault> {
        let declaration = self.declaration();
        self.bounds.before_sending(declaration, [])?;
        let backend = declaration.model().backend();
        let foreign = request
            .messages
            .iter()
            .flat_map(|message| opaque_of(&message.parts))
            .find(|opaque| opaque.adapter != backend);
        if let Some(opaque) = foreign {
            return Err(Fault::new(
                FaultKind::Unsupported,
                &format!(
                    "the conversation holds a part that only the adapter {} reads, and the \
                     backend is {backend}",
                    opaque.adapter
                ),
            ));
        }
        let image = request.has_image().then_some(feature::IMAGE);
        let tool = (!request.tools.is_empty()).then_some(feature::TOOL);
        self.bounds
            .before_sending(declaration, image.into_iter().chain(tool))?;
        let mut sent = self.bounds.sending(Capability::Judge, declaration)?;
        // What was sent is written before it is sent, when the run keeps its
        // exchanges: a request whose exchange the run has no room for is not sent.
        let head = sent.as_ref().and_then(|sent| sent.head(request));
        if let Some((sent, head)) = sent.as_mut().zip(head.as_ref())
            && let Err(fault) = sent.room(head)
        {
            let refused = Err(fault);
            sent.ended(&refused);
            sent.judged(None, request, &refused, None);
            return refused;
        }
        let answer = self.backend.send(request, &self.bounds).and_then(|turn| {
            if turn.opaque().any(|opaque| opaque.adapter != backend) {
                return Err(Unfit::NotAllowed {
                    what: "a turn",
                    why: "holds a part of another adapter than the backend that answered",
                }
                .into());
            }
            Ok(turn)
        });
        // The provider's own words, in a turn or in a fault, have the run's secrets
        // taken out while they are whole, before anything keeps them or is told them.
        let redactor = self.bounds.redactor();
        let answer = match (answer, &redactor) {
            (Ok(turn), Some(recorder)) => Ok(turn.redacted(&|text| recorder.redact(text))),
            (Err(fault), Some(recorder)) => Err(fault.redacted(&|text| recorder.redact(text))),
            (answer, None) => answer,
        };
        // A turn over the limit this handle was given is not taken, and not stored.
        let over = |turn: &Turn| {
            self.turn_bytes
                .is_some_and(|limit| turn.text_bytes() > limit)
        };
        let (answer, unwritten) = match answer {
            Ok(turn) if over(&turn) => {
                let fault = Fault::new(
                    FaultKind::TooLarge,
                    &format!(
                        "the turn holds {} bytes of text, and at most {} are taken",
                        turn.text_bytes(),
                        self.turn_bytes.unwrap_or(0)
                    ),
                );
                (Err(fault), Some(turn))
            }
            answer => (answer, None),
        };
        if let Some(sent) = &mut sent {
            sent.ended(&answer);
            sent.judged(head.as_ref(), request, &answer, unwritten.as_ref());
        }
        answer
    }

    /// Says that the answer to the last request the run sent to a judge was left out
    /// of the conversation that went on, and why: the harness's decision, which the
    /// run's record keeps with the turn (ADR 0009). It is for a loop that sends one
    /// request at a time. False when the handle keeps nothing, or no judge was asked.
    pub fn exclude_last(&self, why: Excluded) -> bool {
        self.bounds.exclude_last(why)
    }
}

/// Why the harness left a judge's answer out of the conversation that went on. The
/// words are those of the baseline's transcripts (`docs/baseline/transcript.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Excluded {
    /// The answer was cut off at the most output tokens.
    Truncated,
    /// The model declined.
    Refusal,
    /// The answer called no tool and gave no verdict.
    NoCall,
}

impl Excluded {
    /// Every reason, in the order above.
    pub const ALL: [Excluded; 3] = [Excluded::Truncated, Excluded::Refusal, Excluded::NoCall];
}

/// What a run's record says of one exchange with a judge (ADR 0009): a summary within
/// a bound. The whole exchange is a diagnostic of the run, named here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Judged {
    /// The name, among the run's names, of the diagnostic that holds the exchange.
    pub(crate) exchange: Option<String>,
    pub(crate) max_output_tokens: u32,
    /// How many messages were sent.
    pub(crate) messages: usize,
    /// Why the turn ended, as a word; none when no turn came back.
    pub(crate) stop: Option<&'static str>,
    /// How many tools the turn called.
    pub(crate) tool_calls: usize,
    pub(crate) usage: Option<Usage>,
    /// The provider's name for the exchange, cleaned when the turn was made.
    pub(crate) provider_id: Option<String>,
    pub(crate) excluded: Option<Excluded>,
}

impl Judged {
    /// What is kept of a request whose backend's call unwound: nothing of it.
    pub(crate) fn unanswered() -> Judged {
        Judged {
            exchange: None,
            max_output_tokens: 0,
            messages: 0,
            stop: None,
            tool_calls: 0,
            usage: None,
            provider_id: None,
            excluded: None,
        }
    }

    pub(crate) fn of(
        request: &JudgeRequest,
        answer: &Result<Turn, Fault>,
        exchange: Option<String>,
    ) -> Judged {
        let turn = answer.as_ref().ok();
        Judged {
            exchange,
            max_output_tokens: request.controls.max_output_tokens,
            messages: request.messages.len(),
            stop: turn.map(|turn| turn.stop.word()),
            tool_calls: turn.map_or(0, |turn| turn.tool_calls().count()),
            usage: turn.and_then(Turn::usage),
            provider_id: turn.and_then(|turn| turn.request_id.clone()),
            excluded: None,
        }
    }
}

impl StopReason {
    /// The reason as a word. Another reason is `other`: the provider's own word is in
    /// the turn.
    pub fn word(&self) -> &'static str {
        match self {
            StopReason::EndTurn => "end_turn",
            StopReason::ToolUse => "tool_use",
            StopReason::MaxTokens => "max_tokens",
            StopReason::Refusal => "refusal",
            StopReason::Other(_) => "other",
        }
    }
}

impl std::fmt::Debug for JudgeHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "JudgeHandle({:?})", self.declaration())
    }
}
