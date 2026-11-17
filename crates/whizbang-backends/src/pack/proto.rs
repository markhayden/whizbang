//! The pack protocol, `pack/1` (ADR 0020): its messages, and the one codec that writes
//! and reads them as frames.
//!
//! A frame is four bytes of a header's length, the header, and each payload the header
//! describes after eight bytes of its length. The header is one JSON object in one
//! canonical writing; a payload is raw samples. [`write`] writes a frame and [`read`]
//! reads one, strictly: every bound is looked at before the bytes it bounds are read,
//! nothing is set aside for a frame that is refused, and a refusal ([`Refused`]) is one
//! reason of a closed list and a sentence of fixed words and numbers, which quotes
//! nothing the other side sent.
//!
//! The walk over a header is by hand, from its bytes, so that every refusal is made
//! here and none depends on how a JSON library was built: serde_json's
//! `arbitrary_precision`, which another crate of the workspace turns on, would hand
//! `-0` over as 0 and a number with a fraction as an object. Nothing here starts a
//! process, holds a queue or knows a deadline: that is the supervisor's (tasks P3a to
//! P4).

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{Read, Write};

use whizbang_core::capability::{
    Asked, FAULT_LIMIT, FaultKind, INSTANCE_LIMIT, Instance, PIXEL_LIMIT, QUERY_LIMIT, Query,
    SegmentRequest, Unfit,
};
use whizbang_core::contract::Secrets;
use whizbang_core::identity::{BindingId, Digest};

use crate::models::{InstallationId, ModelName};

/// The one version this codec speaks.
pub const VERSION: &str = "pack/1";
/// The most bytes of a header, but for a result's.
pub const HEADER_LIMIT: u64 = 64 * 1024;
/// What a result's header may hold more, for each instance its request allows.
pub const INSTANCE_HEADER_BYTES: u64 = 512;
/// The most bytes of one result's payloads together, whatever its request allows.
pub const RESULT_PAYLOAD_LIMIT: u64 = 1 << 30;
/// The most settings a `load` carries.
pub const SETTING_LIMIT: usize = 32;
/// The most characters of a setting's name.
pub const SETTING_NAME_LIMIT: usize = 64;
/// The most characters of a setting's text.
pub const SETTING_TEXT_LIMIT: usize = 1_024;
/// The most events one request may be sent.
pub const EVENT_LIMIT: u32 = 256;
/// The most bytes of text one request may be sent beside its result: the text of
/// every event and the result's diagnostics, together.
pub const DIAGNOSTICS_LIMIT: u64 = 64 * 1024;
/// The most characters of an id.
pub const ID_LIMIT: usize = 64;
/// The most characters of a device's name.
pub const DEVICE_LIMIT: usize = 32;
/// The most threads a worker says it holds.
pub const THREADS_LIMIT: u32 = 65_535;

/// The most instances any result holds: ADR 0006's most queries, each with its most.
const INSTANCES_MOST: u64 = QUERY_LIMIT as u64 * INSTANCE_LIMIT as u64;

// --- refusals ------------------------------------------------------------------------------------

/// Why a frame is refused: one of a closed list (ADR 0020, "What a refusal says").
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Reason {
    /// The input ended, or failed, inside a frame.
    Truncated,
    /// A header's stated length is 0 or over its bound.
    HeaderTooLarge,
    /// The header is not UTF-8, is not one JSON object, or gives a field twice.
    NotJson,
    /// `type` is none of the six.
    UnknownMessage,
    /// A hello states another version.
    Version,
    /// A request or a result is of a kind `pack/1` has not.
    UnknownKind,
    /// An object holds a field that is not its own.
    UnknownField,
    /// An object lacks a field of its own.
    MissingField,
    /// A value is of the wrong type, breaks its field's rule, or is not finite.
    BadValue,
    /// A count or a size is over its bound.
    OverBound,
    /// Settings or instances are not in their order.
    OutOfOrder,
    /// The header is not the canonical writing of what it holds.
    NotCanonical,
    /// A result or an event for a request that is not open, or of another kind.
    Unexpected,
    /// A plane is not the size of its request's image.
    WrongSize,
    /// A plane names another binding than its request's image is under.
    WrongBinding,
    /// A result's payloads together are over [`RESULT_PAYLOAD_LIMIT`].
    TooLarge,
    /// A payload's stated length is not what its entry gives.
    WrongLength,
}

impl Reason {
    /// Every reason, in the order ADR 0020 lists them.
    pub const ALL: [Reason; 17] = [
        Reason::Truncated,
        Reason::HeaderTooLarge,
        Reason::NotJson,
        Reason::UnknownMessage,
        Reason::Version,
        Reason::UnknownKind,
        Reason::UnknownField,
        Reason::MissingField,
        Reason::BadValue,
        Reason::OverBound,
        Reason::OutOfOrder,
        Reason::NotCanonical,
        Reason::Unexpected,
        Reason::WrongSize,
        Reason::WrongBinding,
        Reason::TooLarge,
        Reason::WrongLength,
    ];

    /// The reason's name, as a vector and the pack's own codec spell it.
    pub fn name(self) -> &'static str {
        match self {
            Reason::Truncated => "truncated",
            Reason::HeaderTooLarge => "header_too_large",
            Reason::NotJson => "not_json",
            Reason::UnknownMessage => "unknown_message",
            Reason::Version => "version",
            Reason::UnknownKind => "unknown_kind",
            Reason::UnknownField => "unknown_field",
            Reason::MissingField => "missing_field",
            Reason::BadValue => "bad_value",
            Reason::OverBound => "over_bound",
            Reason::OutOfOrder => "out_of_order",
            Reason::NotCanonical => "not_canonical",
            Reason::Unexpected => "unexpected",
            Reason::WrongSize => "wrong_size",
            Reason::WrongBinding => "wrong_binding",
            Reason::TooLarge => "too_large",
            Reason::WrongLength => "wrong_length",
        }
    }
}

/// A frame that was refused: the reason, and a sentence of fixed words and numbers.
/// It quotes nothing the other side sent, but for the name of another version when
/// that is a plain name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    reason: Reason,
    sentence: String,
    /// Whether the bound that was broken is a count of a result's instances.
    instances: bool,
}

impl Refused {
    fn new(reason: Reason, sentence: impl Into<String>) -> Refused {
        Refused {
            reason,
            sentence: sentence.into(),
            instances: false,
        }
    }

    pub fn reason(&self) -> Reason {
        self.reason
    }

    /// Whether the bound that was broken is a count of a result's instances: more for one
    /// query than its request allows, or more than any request allows. Such an
    /// `over_bound` ends its call as `too_large`, and every other as `malformed` (ADR
    /// 0020, "What a refusal says").
    pub fn counts_instances(&self) -> bool {
        self.instances
    }

    pub fn sentence(&self) -> &str {
        &self.sentence
    }
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.sentence)
    }
}

impl std::error::Error for Refused {}

fn bad(what: &str, rule: &str) -> Refused {
    Refused::new(Reason::BadValue, format!("{what} is not {rule}"))
}

fn over(what: &str, size: u64, limit: u64) -> Refused {
    Refused::new(
        Reason::OverBound,
        format!("{what} is {size}, over the bound of {limit}"),
    )
}

/// An `over_bound` of a count of a result's instances.
fn over_instances(what: &str, size: u64, limit: u64) -> Refused {
    Refused {
        instances: true,
        ..over(what, size, limit)
    }
}

/// Why a message was not written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unwritten {
    /// The message breaks a rule of the protocol. Nothing of it was written.
    Refused(Refused),
    /// The output failed. Part of the frame may have been written.
    Output(std::io::ErrorKind),
}

impl From<Refused> for Unwritten {
    fn from(refused: Refused) -> Unwritten {
        Unwritten::Refused(refused)
    }
}

// --- the messages --------------------------------------------------------------------------------

/// One message of `pack/1`. A value of this type is plain data: [`write`] refuses one
/// that breaks a rule, and [`read`] hands back none that does.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    /// The first frame each way. Its id names the conversation.
    Hello { id: String, role: Role },
    /// Something asked. Its id is one its sender has not open.
    Request { id: String, ask: Ask },
    /// The end of a request that was answered.
    Result {
        id: String,
        /// The device that answered.
        device: String,
        /// How many threads the worker's process holds, after the call.
        threads: u32,
        /// What the adapter measured, in its own words: a text of the protocol.
        diagnostics: String,
        answer: Answer,
    },
    /// Something a call says before it ends.
    Event { id: String, text: String },
    /// Asks that the request with this id stop.
    Cancel { id: String },
    /// The end of a request that was not answered; with the hello's id, the end of
    /// the conversation.
    Error {
        id: String,
        fault: FaultKind,
        sentence: String,
    },
}

/// Which side says hello, and what a worker says of itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    Whizbang,
    Worker {
        /// The device a model's work runs on; none while the worker has not asked
        /// its runtime.
        device: Option<String>,
        /// How many threads the worker's process holds now.
        threads: u32,
    },
}

/// What a request asks.
#[derive(Debug, Clone, PartialEq)]
pub enum Ask {
    /// Make this installation of this model the resident one, with these settings.
    Load {
        model: ModelName,
        installation: InstallationId,
        settings: BTreeMap<String, Scalar>,
    },
    /// Let the resident model go.
    Unload {
        model: ModelName,
        installation: InstallationId,
    },
    /// Segment (ADR 0006): one image, under its binding, and a batch of queries.
    Segment {
        model: ModelName,
        installation: InstallationId,
        /// The canonical pixels, four samples a pixel.
        image: Raster,
        queries: Vec<Query>,
        max_instances: u32,
    },
}

impl Ask {
    /// The `segment` that carries this request of the core's to this installation of
    /// this model. The pixels are copied.
    pub fn segment(
        request: &SegmentRequest,
        model: ModelName,
        installation: InstallationId,
    ) -> Ask {
        let image = Raster {
            binding: request.binding(),
            width: request.image().width(),
            height: request.image().height(),
            samples: request.image().rgba().to_vec(),
        };
        Ask::Segment {
            model,
            installation,
            image,
            queries: request.queries().to_vec(),
            max_instances: request.max_instances(),
        }
    }

    fn kind(&self) -> Kind {
        match self {
            Ask::Load { .. } => Kind::Load,
            Ask::Unload { .. } => Kind::Unload,
            Ask::Segment { .. } => Kind::Segment,
        }
    }
}

/// The value of one setting of a model, as the config has one (ADR 0011).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scalar {
    Text(String),
    Whole(i64),
    Flag(bool),
}

/// What a result answers.
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    Load,
    Unload,
    /// The instances found. They are written in the fixed order, whatever order they
    /// are in here, and are read in it.
    Segment(Vec<Found>),
}

impl Answer {
    fn kind(&self) -> Kind {
        match self {
            Answer::Load => Kind::Load,
            Answer::Unload => Kind::Unload,
            Answer::Segment(_) => Kind::Segment,
        }
    }
}

/// One instance of an answer, as the wire holds it.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    /// The place, in the request's queries, of the query it answers.
    pub query: u32,
    /// From 0 to 1.
    pub score: f64,
    /// The model's predicted IoU, from 0 to 1, when it gives one.
    pub iou: Option<f64>,
    /// Left, top, right and bottom, in pixels of the image.
    pub bounds: [f64; 4],
    /// One sample a pixel: 255 on the thing, 0 off it.
    pub plane: Raster,
}

/// A payload and what its entry states of it: raw samples, eight bits each, row by row
/// from the top left, and the binding they belong to. A request's image is four samples
/// a pixel and an instance's plane is one; which it is follows from where it is. What
/// `{:?}` prints of one is its size and its binding, never its samples.
#[derive(Clone, PartialEq, Eq)]
pub struct Raster {
    pub binding: BindingId,
    pub width: u32,
    pub height: u32,
    pub samples: Vec<u8>,
}

impl std::fmt::Debug for Raster {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Raster({} by {}, {} bytes, {})",
            self.width,
            self.height,
            self.samples.len(),
            self.binding
        )
    }
}

/// The instances of an answer as the core holds them: for each of the request's
/// queries, in order, its instances in the order they were read. Each is made as the
/// core makes one, so a plane that is not 0 and 255, has nothing on, or is on outside
/// its box is refused here, with the core's own refusal.
pub fn answers(found: Vec<Found>, queries: usize) -> Result<Vec<Vec<Instance>>, Unfit> {
    let mut answers = vec![Vec::new(); queries];
    for found in found {
        let Some(answer) = answers.get_mut(found.query as usize) else {
            return Err(Unfit::NotAllowed {
                what: "the segmenter's answer",
                why: "holds an instance for a request that was not asked",
            });
        };
        let Raster {
            binding,
            width,
            height,
            samples,
        } = found.plane;
        let instance = Instance::new(binding, width, height, samples, found.bounds, found.score)?;
        answer.push(instance);
    }
    Ok(answers)
}

/// The fixed order of an answer: best score first; then by the query; then by the
/// box's top, its left, its right and its bottom; then by the predicted IoU, none
/// first; then by the plane's bytes. It is total: two instances it finds equal are
/// equal in every byte a result holds of them.
fn in_order(one: &Found, other: &Found) -> Ordering {
    by_header(one, other).then_with(|| one.plane.samples.cmp(&other.plane.samples))
}

/// The part of the fixed order that a header alone gives: every key but the plane's
/// bytes. Numbers with a fraction are compared by IEEE 754's total order: by value,
/// and -0 before 0. A header holds no number that is not finite.
fn by_header(one: &Found, other: &Found) -> Ordering {
    let number = |a: f64, b: f64| a.total_cmp(&b);
    let iou = match (one.iou, other.iou) {
        (Some(one), Some(other)) => number(one, other),
        (one, other) => one.is_some().cmp(&other.is_some()),
    };
    number(other.score, one.score)
        .then(one.query.cmp(&other.query))
        .then(number(one.bounds[1], other.bounds[1]))
        .then(number(one.bounds[0], other.bounds[0]))
        .then(number(one.bounds[2], other.bounds[2]))
        .then(number(one.bounds[3], other.bounds[3]))
        .then(iou)
}

/// The instances in the fixed order, whatever order they were built in.
fn ordered(found: &[Found]) -> Vec<&Found> {
    let mut ordered: Vec<&Found> = found.iter().collect();
    ordered.sort_by(|one, other| in_order(one, other));
    ordered
}

// --- what a reader has open ----------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Load,
    Unload,
    Segment,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Load => "load",
            Kind::Unload => "unload",
            Kind::Segment => "segment",
        }
    }

    fn named(name: &str) -> Option<Kind> {
        [Kind::Load, Kind::Unload, Kind::Segment]
            .into_iter()
            .find(|kind| kind.name() == name)
    }
}

/// What the answer to a `segment` is held to: its request's image, and how many
/// instances the request allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Allowed {
    width: u32,
    height: u32,
    binding: BindingId,
    queries: u32,
    max_instances: u32,
}

/// What one side keeps for a request it has open: what the request's answer is held
/// to, worked out from the request, and how much the request was already sent beside
/// its result (ADR 0020, "Bounds").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Awaited {
    kind: Kind,
    allowed: Option<Allowed>,
    events: u32,
    text: u64,
}

impl Awaited {
    fn new(kind: Kind, allowed: Option<Allowed>) -> Awaited {
        Awaited {
            kind,
            allowed,
            events: 0,
            text: 0,
        }
    }

    /// For a `load`.
    pub fn load() -> Awaited {
        Awaited::new(Kind::Load, None)
    }

    /// For an `unload`.
    pub fn unload() -> Awaited {
        Awaited::new(Kind::Unload, None)
    }

    /// For a `segment` of an image of this size under this binding, with this many
    /// queries and at most this many instances for each. The two counts are held to
    /// ADR 0006's most, 64 each, whatever is given.
    pub fn segment(
        width: u32,
        height: u32,
        binding: BindingId,
        queries: u32,
        max_instances: u32,
    ) -> Awaited {
        let allowed = Allowed {
            width,
            height,
            binding,
            queries: queries.min(QUERY_LIMIT as u32),
            max_instances: max_instances.min(INSTANCE_LIMIT),
        };
        Awaited::new(Kind::Segment, Some(allowed))
    }

    /// For this request.
    pub fn of(ask: &Ask) -> Awaited {
        match ask {
            Ask::Load { .. } => Awaited::load(),
            Ask::Unload { .. } => Awaited::unload(),
            Ask::Segment {
                image,
                queries,
                max_instances,
                ..
            } => Awaited::segment(
                image.width,
                image.height,
                image.binding,
                queries.len() as u32,
                *max_instances,
            ),
        }
    }

    /// The same, for a request that was already sent this many events and this many
    /// bytes of text: how a reader that takes over a conversation, as a vector's
    /// does, is told what was counted before it.
    pub fn after(mut self, events: u32, text: u64) -> Awaited {
        self.events = events;
        self.text = text;
        self
    }

    /// The most bytes of the header of this request's result: [`HEADER_LIMIT`], and
    /// [`INSTANCE_HEADER_BYTES`] for each instance the request allows.
    pub fn header_bound(&self) -> u64 {
        let instances = self.allowed.map_or(0, |allowed| {
            u64::from(allowed.queries) * u64::from(allowed.max_instances)
        });
        HEADER_LIMIT + INSTANCE_HEADER_BYTES * instances
    }

    /// How many events the request was sent.
    pub fn events(&self) -> u32 {
        self.events
    }

    /// How many bytes of text the request's events held.
    pub fn text(&self) -> u64 {
        self.text
    }
}

/// The requests one side has open, by id. A side notes a request here before it
/// writes the request's frame; a result or an error that is read closes it.
#[derive(Debug, Default)]
pub struct Open(BTreeMap<String, Awaited>);

impl Open {
    pub fn new() -> Open {
        Open::default()
    }

    /// Notes a request as open under this id. An id is open once.
    pub fn expect(&mut self, id: &str, awaited: Awaited) -> Result<(), Refused> {
        check_id(id)?;
        if self.0.contains_key(id) {
            return Err(Refused::new(
                Reason::Unexpected,
                "a request with that id is already open",
            ));
        }
        self.0.insert(id.to_owned(), awaited);
        Ok(())
    }

    /// What the request with this id is held to, while it is open.
    pub fn awaited(&self, id: &str) -> Option<&Awaited> {
        self.0.get(id)
    }

    /// The most bytes of the next header: the largest bound among what is open.
    fn header_bound(&self) -> u64 {
        let largest = self.0.values().map(Awaited::header_bound).max();
        largest.unwrap_or(HEADER_LIMIT).max(HEADER_LIMIT)
    }
}

// --- rules of single values ----------------------------------------------------------------------

fn lower(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit()
}

/// 1 to `limit` characters: one `first` takes, then any of those `rest` takes.
fn named(text: &str, limit: usize, first: fn(u8) -> bool, rest: fn(u8) -> bool) -> bool {
    text.len() <= limit && text.bytes().next().is_some_and(first) && text.bytes().all(rest)
}

fn check_id(id: &str) -> Result<(), Refused> {
    let rest = |byte: u8| lower(byte) || b"._-".contains(&byte);
    match named(id, ID_LIMIT, lower, rest) {
        true => Ok(()),
        false => Err(bad(
            "an id",
            "a name of lower-case letters, digits and . _ -",
        )),
    }
}

fn check_device(device: &str) -> Result<(), Refused> {
    let first = |byte: u8| byte.is_ascii_lowercase();
    let rest = |byte: u8| lower(byte) || b"._-:".contains(&byte);
    match named(device, DEVICE_LIMIT, first, rest) {
        true => Ok(()),
        false => Err(bad(
            "a device",
            "a name of lower-case letters, digits and . _ - :",
        )),
    }
}

fn check_threads(threads: u32) -> Result<(), Refused> {
    match (1..=THREADS_LIMIT).contains(&threads) {
        true => Ok(()),
        false => Err(bad("a count of threads", "from 1 to 65535")),
    }
}

/// A text of the protocol: what a fault's sentence may hold after cleaning (ADR 0006),
/// of `least` to `most` characters. A text over `most` is `over`, when that is a
/// bound that is counted, and otherwise a bad value.
fn check_text(what: &str, text: &str, least: usize, most: u64) -> Result<(), Refused> {
    let kept = |byte: u8| byte.is_ascii_alphanumeric() || b" .,;:'()%+-_".contains(&byte);
    if text.len() < least || !text.bytes().all(kept) {
        return Err(bad(
            what,
            "a text of ASCII letters, digits, spaces and . , ; : ' ( ) % + - _",
        ));
    }
    match text.len() as u64 > most {
        true if most == DIAGNOSTICS_LIMIT => Err(over(what, text.len() as u64, most)),
        true => Err(bad(what, "within its length")),
        false => Ok(()),
    }
}

/// Whether a `load` may carry this setting of a model: its name a name of lower-case
/// letters, digits and `_`, 64 characters at most; a text of 1 to 1,024 printable ASCII
/// characters with no space at either end and no separator, `/` or `\`. The kind `pack`
/// asks it where the config is read (task P6a), so that no `load` is refused for a
/// setting when it is written.
pub fn check_setting(name: &str, value: &Scalar) -> Result<(), Refused> {
    let first = |byte: u8| byte.is_ascii_lowercase();
    let rest = |byte: u8| lower(byte) || byte == b'_';
    if !named(name, SETTING_NAME_LIMIT, first, rest) {
        return Err(bad(
            "a setting's name",
            "a name of lower-case letters, digits and _",
        ));
    }
    let Scalar::Text(text) = value else {
        return Ok(());
    };
    let printable = |byte: u8| (0x20..=0x7e).contains(&byte) && byte != b'/' && byte != b'\\';
    let plain = (1..=SETTING_TEXT_LIMIT).contains(&text.len())
        && text.bytes().all(printable)
        && !text.starts_with(' ')
        && !text.ends_with(' ');
    match plain {
        true => Ok(()),
        false => Err(bad(
            "a setting's text",
            "1 to 1024 printable ASCII characters with no space at either end and no separator",
        )),
    }
}

/// How many samples a pixel of a layout holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Layout {
    /// A request's image: red, green, blue and straight alpha, sRGB.
    Rgba,
    /// An instance's plane.
    Plane,
}

impl Layout {
    /// What an entry of this layout states: its layout, data type, colour and alpha.
    fn words(self) -> [&'static str; 4] {
        match self {
            Layout::Rgba => ["rgba", "u8", "srgb", "straight"],
            Layout::Plane => ["plane", "u8", "none", "none"],
        }
    }

    fn samples(self) -> u64 {
        match self {
            Layout::Rgba => 4,
            Layout::Plane => 1,
        }
    }

    /// How many bytes a payload of this layout and this size is.
    fn bytes(self, raster: &Raster) -> u64 {
        self.samples() * u64::from(raster.width) * u64::from(raster.height)
    }
}

/// What an entry states: at least one pixel, and [`PIXEL_LIMIT`] at most.
fn check_entry(raster: &Raster) -> Result<(), Refused> {
    let pixels = u64::from(raster.width) * u64::from(raster.height);
    if pixels == 0 {
        return Err(bad("a payload's width or height", "at least 1"));
    }
    match pixels > PIXEL_LIMIT {
        true => Err(over("a payload's count of pixels", pixels, PIXEL_LIMIT)),
        false => Ok(()),
    }
}

fn check_unit(what: &str, value: f64) -> Result<(), Refused> {
    match (0.0..=1.0).contains(&value) {
        true => Ok(()),
        false => Err(bad(what, "a number from 0 to 1")),
    }
}

/// Every rule a message keeps that its header alone shows.
fn check(message: &Message) -> Result<(), Refused> {
    match message {
        Message::Hello { id, role } => {
            check_id(id)?;
            if let Role::Worker { device, threads } = role {
                device.as_deref().map_or(Ok(()), check_device)?;
                check_threads(*threads)?;
            }
            Ok(())
        }
        Message::Request { id, ask } => {
            check_id(id)?;
            match ask {
                Ask::Load { settings, .. } => {
                    if settings.len() > SETTING_LIMIT {
                        let (count, limit) = (settings.len() as u64, SETTING_LIMIT as u64);
                        return Err(over("the count of settings", count, limit));
                    }
                    settings
                        .iter()
                        .try_for_each(|(name, value)| check_setting(name, value))
                }
                Ask::Unload { .. } => Ok(()),
                Ask::Segment {
                    image,
                    queries,
                    max_instances,
                    ..
                } => {
                    check_entry(image)?;
                    if queries.is_empty() {
                        return Err(bad("the list of queries", "of one query at least"));
                    }
                    if queries.len() > QUERY_LIMIT {
                        let (count, limit) = (queries.len() as u64, QUERY_LIMIT as u64);
                        return Err(over("the count of queries", count, limit));
                    }
                    if *max_instances == 0 {
                        return Err(bad("the most instances", "at least 1"));
                    }
                    if *max_instances > INSTANCE_LIMIT {
                        let (count, limit) = (u64::from(*max_instances), u64::from(INSTANCE_LIMIT));
                        return Err(over("the most instances", count, limit));
                    }
                    Ok(())
                }
            }
        }
        Message::Result {
            id,
            device,
            threads,
            diagnostics,
            answer,
        } => {
            check_id(id)?;
            check_device(device)?;
            check_threads(*threads)?;
            check_text("a result's diagnostics", diagnostics, 0, DIAGNOSTICS_LIMIT)?;
            let Answer::Segment(found) = answer else {
                return Ok(());
            };
            if found.len() as u64 > INSTANCES_MOST {
                return Err(over_instances(
                    "the count of instances",
                    found.len() as u64,
                    INSTANCES_MOST,
                ));
            }
            for found in found {
                check_unit("a score", found.score)?;
                found
                    .iou
                    .map_or(Ok(()), |iou| check_unit("a predicted IoU", iou))?;
                if !found.bounds.iter().all(|edge| edge.is_finite()) {
                    return Err(bad("a box", "four finite numbers"));
                }
                check_entry(&found.plane)?;
            }
            Ok(())
        }
        Message::Event { id, text } => {
            check_id(id)?;
            check_text("an event's text", text, 1, DIAGNOSTICS_LIMIT)
        }
        Message::Cancel { id } => check_id(id),
        Message::Error { id, sentence, .. } => {
            check_id(id)?;
            check_text("an error's sentence", sentence, 1, FAULT_LIMIT as u64)
        }
    }
}

/// The payloads of a message, in the order they follow its header, each with its
/// layout.
fn payloads(message: &Message) -> Vec<(Layout, &Raster)> {
    match message {
        Message::Request {
            ask: Ask::Segment { image, .. },
            ..
        } => vec![(Layout::Rgba, image)],
        Message::Result {
            answer: Answer::Segment(found),
            ..
        } => ordered(found)
            .into_iter()
            .map(|found| (Layout::Plane, &found.plane))
            .collect(),
        _ => Vec::new(),
    }
}

// --- the canonical writing -----------------------------------------------------------------------

/// A text, as a header writes one.
fn text(out: &mut String, value: &str) {
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            control if (control as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", control as u32);
            }
            other => out.push(other),
        }
    }
    out.push('"');
}

/// A number with a fraction, as a header writes one: its 64 bits, in hex.
fn bits(out: &mut String, value: f64) {
    let _ = write!(out, "\"{:016x}\"", value.to_bits());
}

/// A list of numbers with a fraction.
fn numbers(out: &mut String, values: &[f64]) {
    out.push('[');
    for (at, value) in values.iter().enumerate() {
        if at > 0 {
            out.push(',');
        }
        bits(out, *value);
    }
    out.push(']');
}

/// An object being written: each field in the order it is given.
struct Fields<'a> {
    out: &'a mut String,
    first: bool,
}

impl<'a> Fields<'a> {
    fn start(out: &'a mut String) -> Fields<'a> {
        out.push('{');
        Fields { out, first: true }
    }

    /// The field's name, after which its value is written.
    fn field(&mut self, name: &str) -> &mut String {
        if !self.first {
            self.out.push(',');
        }
        self.first = false;
        let _ = write!(self.out, "\"{name}\":");
        self.out
    }

    fn text(&mut self, name: &str, value: &str) {
        text(self.field(name), value);
    }

    fn whole(&mut self, name: &str, value: u64) {
        let _ = write!(self.field(name), "{value}");
    }

    fn entry(&mut self, name: &str, layout: Layout, raster: &Raster) {
        let [arrangement, data, colour, alpha] = layout.words();
        let mut entry = Fields::start(self.field(name));
        entry.whole("width", u64::from(raster.width));
        entry.whole("height", u64::from(raster.height));
        entry.text("layout", arrangement);
        entry.text("data", data);
        entry.text("colour", colour);
        entry.text("alpha", alpha);
        entry.text("binding", &raster.binding.to_string());
        entry.end();
    }

    fn end(self) {
        self.out.push('}');
    }
}

/// The header of a message, in the canonical writing.
fn header_of(message: &Message) -> String {
    let mut out = String::new();
    let mut fields = Fields::start(&mut out);
    match message {
        Message::Hello { id, role } => {
            fields.text("type", "hello");
            fields.text("id", id);
            fields.text("version", VERSION);
            match role {
                Role::Whizbang => fields.text("role", "whizbang"),
                Role::Worker { device, threads } => {
                    fields.text("role", "worker");
                    match device {
                        Some(device) => fields.text("device", device),
                        None => fields.field("device").push_str("null"),
                    }
                    fields.whole("threads", u64::from(*threads));
                }
            }
        }
        Message::Request { id, ask } => {
            fields.text("type", "request");
            fields.text("id", id);
            fields.text("kind", ask.kind().name());
            let (Ask::Load {
                model,
                installation,
                ..
            }
            | Ask::Unload {
                model,
                installation,
            }
            | Ask::Segment {
                model,
                installation,
                ..
            }) = ask;
            fields.text("model", model.as_str());
            fields.text("installation", &installation.to_string());
            match ask {
                Ask::Load { settings, .. } => write_settings(fields.field("settings"), settings),
                Ask::Unload { .. } => {}
                Ask::Segment {
                    image,
                    queries,
                    max_instances,
                    ..
                } => {
                    fields.entry("image", Layout::Rgba, image);
                    write_queries(fields.field("queries"), queries);
                    fields.whole("max_instances", u64::from(*max_instances));
                }
            }
        }
        Message::Result {
            id,
            device,
            threads,
            diagnostics,
            answer,
        } => {
            fields.text("type", "result");
            fields.text("id", id);
            fields.text("kind", answer.kind().name());
            fields.text("device", device);
            fields.whole("threads", u64::from(*threads));
            fields.text("diagnostics", diagnostics);
            if let Answer::Segment(found) = answer {
                write_instances(fields.field("instances"), &ordered(found));
            }
        }
        Message::Event { id, text } => {
            fields.text("type", "event");
            fields.text("id", id);
            fields.text("text", text);
        }
        Message::Cancel { id } => {
            fields.text("type", "cancel");
            fields.text("id", id);
        }
        Message::Error {
            id,
            fault,
            sentence,
        } => {
            fields.text("type", "error");
            fields.text("id", id);
            fields.text("fault", fault.name());
            fields.text("sentence", sentence);
        }
    }
    fields.end();
    out
}

fn write_settings(out: &mut String, settings: &BTreeMap<String, Scalar>) {
    out.push('[');
    for (at, (name, value)) in settings.iter().enumerate() {
        if at > 0 {
            out.push(',');
        }
        let mut setting = Fields::start(out);
        setting.text("name", name);
        match value {
            Scalar::Text(value) => setting.text("value", value),
            Scalar::Whole(value) => {
                let _ = write!(setting.field("value"), "{value}");
            }
            Scalar::Flag(value) => {
                let _ = write!(setting.field("value"), "{value}");
            }
        }
        setting.end();
    }
    out.push(']');
}

fn write_queries(out: &mut String, queries: &[Query]) {
    out.push('[');
    for (at, query) in queries.iter().enumerate() {
        if at > 0 {
            out.push(',');
        }
        let mut fields = Fields::start(out);
        match query.asked() {
            Asked::Phrase(phrase) => fields.text("phrase", phrase),
            Asked::Point { x, y } => numbers(fields.field("point"), &[x, y]),
            Asked::Box {
                left,
                top,
                right,
                bottom,
            } => numbers(fields.field("box"), &[left, top, right, bottom]),
        }
        fields.end();
    }
    out.push(']');
}

fn write_instances(out: &mut String, found: &[&Found]) {
    out.push('[');
    for (at, found) in found.iter().enumerate() {
        if at > 0 {
            out.push(',');
        }
        let mut fields = Fields::start(out);
        fields.whole("query", u64::from(found.query));
        bits(fields.field("score"), found.score);
        match found.iou {
            Some(iou) => bits(fields.field("iou"), iou),
            None => fields.field("iou").push_str("null"),
        }
        numbers(fields.field("box"), &found.bounds);
        fields.entry("plane", Layout::Plane, &found.plane);
        fields.end();
    }
    out.push(']');
}

// --- writing a frame -----------------------------------------------------------------------------

/// Writes one message as a frame, and flushes. A message that breaks a rule of the
/// protocol is refused before a byte is written: a value out of its rule, a count over
/// its bound, a header over [`HEADER_LIMIT`] (for a `segment`'s result, over what the
/// most instances any request allows would give: the writer does not know its
/// request), a payload that is not as long as its entry states, a result over
/// [`RESULT_PAYLOAD_LIMIT`]. The instances of a result are
/// written in the fixed order, whatever order they are in.
pub fn write(message: &Message, out: &mut impl Write) -> Result<(), Unwritten> {
    let (header, payloads) = framed(message)?;
    let failed = |error: std::io::Error| Unwritten::Output(error.kind());
    out.write_all(&(header.len() as u32).to_be_bytes())
        .map_err(failed)?;
    out.write_all(header.as_bytes()).map_err(failed)?;
    for (_, raster) in payloads {
        out.write_all(&(raster.samples.len() as u64).to_be_bytes())
            .map_err(failed)?;
        out.write_all(&raster.samples).map_err(failed)?;
    }
    out.flush().map_err(failed)
}

/// Whether [`write`] would write this message: the refusal it would give, or nothing.
/// Nothing is written, so a side that must know before it sends (the supervisor, which
/// notes a request as open before it writes it) asks this first.
pub fn writable(message: &Message) -> Result<(), Refused> {
    framed(message).map(|_| ())
}

/// A message as a frame is written: its header in its canonical writing, and its
/// payloads, each with its layout, in the order they follow the header.
type Framed<'a> = (String, Vec<(Layout, &'a Raster)>);

/// The header of a message in its canonical writing, and its payloads, once every rule
/// [`write`] holds a message to is kept.
fn framed(message: &Message) -> Result<Framed<'_>, Refused> {
    check(message)?;
    let payloads = payloads(message);
    let is_result = matches!(message, Message::Result { .. });
    let together: u64 = payloads
        .iter()
        .map(|(layout, raster)| layout.bytes(raster))
        .sum();
    if is_result && together > RESULT_PAYLOAD_LIMIT {
        return Err(too_large(together));
    }
    for (layout, raster) in &payloads {
        let (stated, expected) = (raster.samples.len() as u64, layout.bytes(raster));
        if stated != expected {
            return Err(wrong_length(stated, expected));
        }
    }
    let header = header_of(message);
    let bound = match message {
        Message::Result {
            answer: Answer::Segment(_),
            ..
        } => HEADER_LIMIT + INSTANCE_HEADER_BYTES * INSTANCES_MOST,
        _ => HEADER_LIMIT,
    };
    if header.len() as u64 > bound {
        return Err(header_too_large(header.len() as u64, bound));
    }
    Ok((header, payloads))
}

fn wrong_length(stated: u64, expected: u64) -> Refused {
    Refused::new(
        Reason::WrongLength,
        format!("a payload is {stated} bytes, and its entry gives {expected}"),
    )
}

fn too_large(together: u64) -> Refused {
    Refused::new(
        Reason::TooLarge,
        format!(
            "a result's payloads are {together} bytes together, over the bound of \
             {RESULT_PAYLOAD_LIMIT}"
        ),
    )
}

fn header_too_large(length: u64, bound: u64) -> Refused {
    Refused::new(
        Reason::HeaderTooLarge,
        format!("a header is {length} bytes, and its bound is {bound}"),
    )
}

// --- reading a header ----------------------------------------------------------------------------

/// A JSON value, read strictly: an object that gives a field twice is refused where
/// it is read. A whole number is kept when 64 bits hold it; of any other number, only
/// that it was one.
enum Json {
    Null,
    Flag(bool),
    Whole(u64),
    Negative(i64),
    Fraction,
    Text(String),
    List(Vec<Json>),
    Object(BTreeMap<String, Json>),
}

/// The most objects and lists a header holds one inside another, its own object the
/// first (ADR 0020, "What a reader takes as JSON").
const DEPTH_LIMIT: usize = 127;

/// A header that is not JSON, strictly (ADR 0020, "What a reader takes as JSON").
struct NotJson;

/// Reads a header as JSON, strictly, by hand: what a value is never depends on how a
/// library was built. The bytes are strict UTF-8, and then one JSON value with
/// nothing after it but white space.
fn json(header: &[u8]) -> Result<Json, NotJson> {
    let text = std::str::from_utf8(header).map_err(|_| NotJson)?;
    let mut walk = Walk {
        bytes: text.as_bytes(),
        at: 0,
    };
    let value = walk.value(0)?;
    walk.space();
    match walk.at == walk.bytes.len() {
        true => Ok(value),
        false => Err(NotJson),
    }
}

/// A walk over the bytes of a header that are known to be UTF-8.
struct Walk<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Walk<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn next(&mut self) -> Result<u8, NotJson> {
        let byte = self.peek().ok_or(NotJson)?;
        self.at += 1;
        Ok(byte)
    }

    fn space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn word(&mut self, word: &[u8], value: Json) -> Result<Json, NotJson> {
        match self.bytes[self.at..].starts_with(word) {
            true => {
                self.at += word.len();
                Ok(value)
            }
            false => Err(NotJson),
        }
    }

    /// A value, inside `depth` objects and lists.
    fn value(&mut self, depth: usize) -> Result<Json, NotJson> {
        self.space();
        match self.peek().ok_or(NotJson)? {
            b'{' | b'[' if depth >= DEPTH_LIMIT => Err(NotJson),
            b'{' => self.object(depth + 1),
            b'[' => self.list(depth + 1),
            b'"' => self.text().map(Json::Text),
            b't' => self.word(b"true", Json::Flag(true)),
            b'f' => self.word(b"false", Json::Flag(false)),
            b'n' => self.word(b"null", Json::Null),
            b'-' | b'0'..=b'9' => self.number(),
            _ => Err(NotJson),
        }
    }

    fn object(&mut self, depth: usize) -> Result<Json, NotJson> {
        self.at += 1;
        let mut fields = BTreeMap::new();
        self.space();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(Json::Object(fields));
        }
        loop {
            self.space();
            if self.peek() != Some(b'"') {
                return Err(NotJson);
            }
            let name = self.text()?;
            self.space();
            if self.next()? != b':' {
                return Err(NotJson);
            }
            let value = self.value(depth)?;
            if fields.insert(name, value).is_some() {
                return Err(NotJson);
            }
            self.space();
            match self.next()? {
                b',' => {}
                b'}' => return Ok(Json::Object(fields)),
                _ => return Err(NotJson),
            }
        }
    }

    fn list(&mut self, depth: usize) -> Result<Json, NotJson> {
        self.at += 1;
        let mut items = Vec::new();
        self.space();
        if self.peek() == Some(b']') {
            self.at += 1;
            return Ok(Json::List(items));
        }
        loop {
            items.push(self.value(depth)?);
            self.space();
            match self.next()? {
                b',' => {}
                b']' => return Ok(Json::List(items)),
                _ => return Err(NotJson),
            }
        }
    }

    /// Four hex digits of a `\u` escape.
    fn unit(&mut self) -> Result<u32, NotJson> {
        let digits = self.bytes.get(self.at..self.at + 4).ok_or(NotJson)?;
        if !digits.iter().all(u8::is_ascii_hexdigit) {
            return Err(NotJson);
        }
        self.at += 4;
        let digits = std::str::from_utf8(digits).map_err(|_| NotJson)?;
        u32::from_str_radix(digits, 16).map_err(|_| NotJson)
    }

    /// A text, from its opening `"`. A control character written as itself, an escape
    /// JSON has not, and a surrogate that is not half of a pair are not JSON.
    fn text(&mut self) -> Result<String, NotJson> {
        self.at += 1;
        let mut text = String::new();
        loop {
            let start = self.at;
            while let Some(byte) = self.peek() {
                if byte == b'"' || byte == b'\\' || byte < 0x20 {
                    break;
                }
                self.at += 1;
            }
            // Only whole characters are passed over: each stop is an ASCII byte.
            text.push_str(std::str::from_utf8(&self.bytes[start..self.at]).map_err(|_| NotJson)?);
            match self.next()? {
                b'"' => return Ok(text),
                b'\\' => {}
                _ => return Err(NotJson),
            }
            let character = match self.next()? {
                b'"' => '"',
                b'\\' => '\\',
                b'/' => '/',
                b'b' => '\u{8}',
                b'f' => '\u{c}',
                b'n' => '\n',
                b'r' => '\r',
                b't' => '\t',
                b'u' => {
                    let first = self.unit()?;
                    let code = match first {
                        0xd800..=0xdbff => {
                            if !self.bytes[self.at..].starts_with(b"\\u") {
                                return Err(NotJson);
                            }
                            self.at += 2;
                            let second = self.unit()?;
                            if !(0xdc00..=0xdfff).contains(&second) {
                                return Err(NotJson);
                            }
                            0x10000 + ((first - 0xd800) << 10) + (second - 0xdc00)
                        }
                        0xdc00..=0xdfff => return Err(NotJson),
                        other => other,
                    };
                    char::from_u32(code).ok_or(NotJson)?
                }
                _ => return Err(NotJson),
            };
            text.push(character);
        }
    }

    /// A number, as JSON writes one. A whole number with no sign, or with one and
    /// under zero, is kept when 64 bits hold it; `-0`, and any number with a fraction
    /// or an exponent or past 64 bits, is only a number; one outside a double's range
    /// is not JSON.
    fn number(&mut self) -> Result<Json, NotJson> {
        let start = self.at;
        let digits = |walk: &mut Walk| {
            let from = walk.at;
            while walk.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                walk.at += 1;
            }
            walk.at - from
        };
        let negative = self.peek() == Some(b'-');
        if negative {
            self.at += 1;
        }
        match self.peek() {
            Some(b'0') => self.at += 1,
            Some(b'1'..=b'9') => {
                digits(self);
            }
            _ => return Err(NotJson),
        }
        let mut whole = true;
        if self.peek() == Some(b'.') {
            self.at += 1;
            whole = false;
            if digits(self) == 0 {
                return Err(NotJson);
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.at += 1;
            whole = false;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            if digits(self) == 0 {
                return Err(NotJson);
            }
        }
        let written = std::str::from_utf8(&self.bytes[start..self.at]).map_err(|_| NotJson)?;
        if whole {
            match negative {
                false => {
                    if let Ok(value) = written.parse::<u64>() {
                        return Ok(Json::Whole(value));
                    }
                }
                true => {
                    if let Ok(value) = written.parse::<i64>()
                        && value < 0
                    {
                        return Ok(Json::Negative(value));
                    }
                }
            }
        }
        match written.parse::<f64>() {
            Ok(value) if value.is_finite() => Ok(Json::Fraction),
            _ => Err(NotJson),
        }
    }
}

/// An object of a header being taken apart. Its fields are known to be exactly its
/// own before any is taken.
struct Taken(BTreeMap<String, Json>);

impl Taken {
    /// The object's fields, when they are exactly these: a field that is not one of
    /// them is refused first, then one of them that is missing.
    fn exactly(fields: BTreeMap<String, Json>, names: &[&'static str]) -> Result<Taken, Refused> {
        if fields.keys().any(|field| !names.contains(&field.as_str())) {
            return Err(unknown_field());
        }
        match names.iter().find(|name| !fields.contains_key(**name)) {
            Some(name) => Err(missing(name)),
            None => Ok(Taken(fields)),
        }
    }

    fn take(&mut self, name: &'static str) -> Json {
        self.0.remove(name).unwrap_or(Json::Null)
    }

    fn text(&mut self, name: &'static str) -> Result<String, Refused> {
        match self.take(name) {
            Json::Text(text) => Ok(text),
            _ => Err(bad(name, "a text")),
        }
    }

    /// A whole number of at most `most`.
    fn whole(&mut self, name: &'static str, most: u64) -> Result<u64, Refused> {
        match self.take(name) {
            Json::Whole(whole) if whole <= most => Ok(whole),
            _ => Err(bad(name, "a whole number in its range")),
        }
    }

    fn list(&mut self, name: &'static str) -> Result<Vec<Json>, Refused> {
        match self.take(name) {
            Json::List(items) => Ok(items),
            _ => Err(bad(name, "a list")),
        }
    }

    fn id(&mut self) -> Result<String, Refused> {
        self.text("id")
    }

    /// An entry that states these rules.
    fn entry(&mut self, name: &'static str, layout: Layout) -> Result<Raster, Refused> {
        let Json::Object(fields) = self.take(name) else {
            return Err(bad(name, "an object"));
        };
        let names = [
            "width", "height", "layout", "data", "colour", "alpha", "binding",
        ];
        let mut entry = Taken::exactly(fields, &names)?;
        let width = entry.whole("width", u64::from(u32::MAX))? as u32;
        let height = entry.whole("height", u64::from(u32::MAX))? as u32;
        for (field, word) in names[2..6].iter().zip(layout.words()) {
            if entry.text(field)? != word {
                return Err(bad(field, "the one rule its entry has"));
            }
        }
        let binding = entry.text("binding")?;
        let binding: Digest = binding.parse().map_err(|_| bad("binding", "an identity"))?;
        Ok(Raster {
            binding: BindingId(binding),
            width,
            height,
            samples: Vec::new(),
        })
    }
}

fn unknown_field() -> Refused {
    Refused::new(
        Reason::UnknownField,
        "an object holds a field that is not its own",
    )
}

fn missing(name: &str) -> Refused {
    Refused::new(Reason::MissingField, format!("the field {name} is missing"))
}

/// A text field of an object whose other fields are not known yet.
fn peek(fields: &BTreeMap<String, Json>, name: &'static str) -> Result<String, Refused> {
    match fields.get(name) {
        Some(Json::Text(text)) => Ok(text.clone()),
        Some(_) => Err(bad(name, "a text")),
        None => Err(missing(name)),
    }
}

/// A number with a fraction, from its 16 lower-case hex digits. That it is finite is
/// its field's rule, held with the others of its message ([`check`], and a query's
/// own constructor).
fn number(json: Json, what: &'static str) -> Result<f64, Refused> {
    let refuse = || bad(what, "a number, as 16 lower-case hex digits");
    let Json::Text(text) = json else {
        return Err(refuse());
    };
    let hex = |byte: u8| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte);
    if text.len() != 16 || !text.bytes().all(hex) {
        return Err(refuse());
    }
    let value = u64::from_str_radix(&text, 16).map(f64::from_bits);
    value.map_err(|_| refuse())
}

/// A list of exactly this many numbers with a fraction.
fn numbers_of<const N: usize>(json: Json, what: &'static str) -> Result<[f64; N], Refused> {
    let Json::List(items) = json else {
        return Err(bad(what, "a list"));
    };
    if items.len() != N {
        return Err(bad(what, "a list of its own length"));
    }
    let mut values = [0.0; N];
    for (value, item) in values.iter_mut().zip(items) {
        *value = number(item, what)?;
    }
    Ok(values)
}

/// A refusal of the core's, for something a header stated.
fn unfit(unfit: Unfit) -> Refused {
    match unfit {
        Unfit::TooLarge { size, limit, .. } => over("a query", size, limit),
        _ => bad("a query", "one the capability takes"),
    }
}

fn read_settings(items: Vec<Json>) -> Result<BTreeMap<String, Scalar>, Refused> {
    let mut settings = BTreeMap::new();
    let mut last: Option<String> = None;
    for item in items {
        let Json::Object(fields) = item else {
            return Err(bad("a setting", "an object"));
        };
        let mut setting = Taken::exactly(fields, &["name", "value"])?;
        let name = setting.text("name")?;
        let value = match setting.take("value") {
            Json::Text(text) => Scalar::Text(text),
            Json::Flag(flag) => Scalar::Flag(flag),
            Json::Negative(whole) => Scalar::Whole(whole),
            Json::Whole(whole) => match i64::try_from(whole) {
                Ok(whole) => Scalar::Whole(whole),
                Err(_) => return Err(bad("a setting's value", "a scalar")),
            },
            _ => return Err(bad("a setting's value", "a scalar")),
        };
        check_setting(&name, &value)?;
        if last.as_ref().is_some_and(|last| *last >= name) {
            return Err(Refused::new(
                Reason::OutOfOrder,
                "the settings are not in the order of their names, each once",
            ));
        }
        last = Some(name.clone());
        settings.insert(name, value);
    }
    Ok(settings)
}

fn read_queries(items: Vec<Json>) -> Result<Vec<Query>, Refused> {
    let mut queries = Vec::new();
    for item in items {
        let Json::Object(mut fields) = item else {
            return Err(bad("a query", "an object"));
        };
        let known = ["phrase", "point", "box"];
        if fields.keys().any(|field| !known.contains(&field.as_str())) {
            return Err(unknown_field());
        }
        if fields.len() > 1 {
            return Err(bad("a query", "one of a phrase, a point and a box"));
        }
        let Some((name, value)) = fields.pop_first() else {
            return Err(missing("phrase, point or box"));
        };
        let query = match (name.as_str(), value) {
            ("phrase", Json::Text(phrase)) => Query::phrase(&phrase),
            ("phrase", _) => return Err(bad("phrase", "a text")),
            ("point", value) => {
                let [x, y] = numbers_of(value, "point")?;
                Query::point(x, y)
            }
            (_, value) => {
                let [left, top, right, bottom] = numbers_of(value, "box")?;
                Query::boxed(left, top, right, bottom)
            }
        };
        queries.push(query.map_err(unfit)?);
    }
    Ok(queries)
}

fn read_instances(items: Vec<Json>) -> Result<Vec<Found>, Refused> {
    let mut instances = Vec::new();
    for item in items {
        let Json::Object(fields) = item else {
            return Err(bad("an instance", "an object"));
        };
        let mut instance = Taken::exactly(fields, &["query", "score", "iou", "box", "plane"])?;
        let query = instance.whole("query", u64::from(u32::MAX))? as u32;
        let score = number(instance.take("score"), "score")?;
        let iou = match instance.take("iou") {
            Json::Null => None,
            other => Some(number(other, "iou")?),
        };
        let bounds = numbers_of(instance.take("box"), "box")?;
        let plane = instance.entry("plane", Layout::Plane)?;
        instances.push(Found {
            query,
            score,
            iou,
            bounds,
            plane,
        });
    }
    Ok(instances)
}

fn read_model(fields: &mut Taken) -> Result<(ModelName, InstallationId), Refused> {
    let secrets = Secrets::none();
    let model = fields.text("model")?;
    let model = ModelName::new(&model, &secrets).map_err(|_| bad("model", "a model's name"))?;
    let installation = fields.text("installation")?;
    let installation = InstallationId::from_text(&installation, &secrets)
        .map_err(|_| bad("installation", "an identity"))?;
    Ok((model, installation))
}

/// The message a header holds, with the samples of its payloads still to be read.
/// The rules are looked at in the order ADR 0020 gives.
fn parse(header: &[u8]) -> Result<Message, Refused> {
    let not_json = || {
        Refused::new(
            Reason::NotJson,
            "a header is not one JSON object with each field once",
        )
    };
    let Ok(Json::Object(fields)) = json(header) else {
        return Err(not_json());
    };
    let message = match peek(&fields, "type")?.as_str() {
        "hello" => {
            let version = peek(&fields, "version")?;
            if version != VERSION {
                return Err(other_version(&version));
            }
            let worker = match peek(&fields, "role")?.as_str() {
                "whizbang" => false,
                "worker" => true,
                _ => return Err(bad("role", "whizbang or worker")),
            };
            let names: &[&'static str] = match worker {
                true => &["type", "id", "version", "role", "device", "threads"],
                false => &["type", "id", "version", "role"],
            };
            let mut fields = Taken::exactly(fields, names)?;
            let id = fields.id()?;
            let role = match worker {
                false => Role::Whizbang,
                true => {
                    let device = match fields.take("device") {
                        Json::Null => None,
                        Json::Text(device) => Some(device),
                        _ => return Err(bad("device", "a text or null")),
                    };
                    let threads = fields.whole("threads", u64::from(u32::MAX))? as u32;
                    Role::Worker { device, threads }
                }
            };
            Message::Hello { id, role }
        }
        "request" => {
            let kind = kind_of(&fields)?;
            let names: &[&'static str] = match kind {
                Kind::Load => &["type", "id", "kind", "model", "installation", "settings"],
                Kind::Unload => &["type", "id", "kind", "model", "installation"],
                Kind::Segment => &[
                    "type",
                    "id",
                    "kind",
                    "model",
                    "installation",
                    "image",
                    "queries",
                    "max_instances",
                ],
            };
            let mut fields = Taken::exactly(fields, names)?;
            let id = fields.id()?;
            let (model, installation) = read_model(&mut fields)?;
            let ask = match kind {
                Kind::Load => Ask::Load {
                    model,
                    installation,
                    settings: read_settings(fields.list("settings")?)?,
                },
                Kind::Unload => Ask::Unload {
                    model,
                    installation,
                },
                Kind::Segment => Ask::Segment {
                    model,
                    installation,
                    image: fields.entry("image", Layout::Rgba)?,
                    queries: read_queries(fields.list("queries")?)?,
                    max_instances: fields.whole("max_instances", u64::from(u32::MAX))? as u32,
                },
            };
            Message::Request { id, ask }
        }
        "result" => {
            let kind = kind_of(&fields)?;
            let names: &[&'static str] = match kind {
                Kind::Segment => &[
                    "type",
                    "id",
                    "kind",
                    "device",
                    "threads",
                    "diagnostics",
                    "instances",
                ],
                _ => &["type", "id", "kind", "device", "threads", "diagnostics"],
            };
            let mut fields = Taken::exactly(fields, names)?;
            let answer = match kind {
                Kind::Load => Answer::Load,
                Kind::Unload => Answer::Unload,
                Kind::Segment => Answer::Segment(read_instances(fields.list("instances")?)?),
            };
            Message::Result {
                id: fields.id()?,
                device: fields.text("device")?,
                threads: fields.whole("threads", u64::from(u32::MAX))? as u32,
                diagnostics: fields.text("diagnostics")?,
                answer,
            }
        }
        "event" => {
            let mut fields = Taken::exactly(fields, &["type", "id", "text"])?;
            Message::Event {
                id: fields.id()?,
                text: fields.text("text")?,
            }
        }
        "cancel" => {
            let mut fields = Taken::exactly(fields, &["type", "id"])?;
            Message::Cancel { id: fields.id()? }
        }
        "error" => {
            let mut fields = Taken::exactly(fields, &["type", "id", "fault", "sentence"])?;
            let id = fields.id()?;
            let fault = fields.text("fault")?;
            let fault = FaultKind::ALL.into_iter().find(|kind| kind.name() == fault);
            Message::Error {
                id,
                fault: fault.ok_or_else(|| bad("fault", "one of the eight faults"))?,
                sentence: fields.text("sentence")?,
            }
        }
        _ => {
            return Err(Refused::new(
                Reason::UnknownMessage,
                "a message is of none of the six types",
            ));
        }
    };
    check(&message)?;
    if let Message::Result {
        answer: Answer::Segment(found),
        ..
    } = &message
    {
        let sorted = found
            .windows(2)
            .all(|pair| by_header(&pair[0], &pair[1]) != Ordering::Greater);
        if !sorted {
            return Err(out_of_order());
        }
    }
    Ok(message)
}

fn out_of_order() -> Refused {
    Refused::new(
        Reason::OutOfOrder,
        "the instances of a result are not in the fixed order",
    )
}

fn kind_of(fields: &BTreeMap<String, Json>) -> Result<Kind, Refused> {
    Kind::named(&peek(fields, "kind")?).ok_or_else(|| {
        Refused::new(
            Reason::UnknownKind,
            "a request or a result is of a kind this version has not",
        )
    })
}

/// The refusal of a hello of another version. It says both, and quotes the other
/// only when it is a plain name of 32 characters at most.
fn other_version(theirs: &str) -> Refused {
    let rest = |byte: u8| lower(byte) || b"./_-".contains(&byte);
    let theirs = match named(theirs, 32, lower, rest) {
        true => theirs,
        false => "a version that is not a name",
    };
    Refused::new(
        Reason::Version,
        format!("this side speaks {VERSION} and the other said {theirs}"),
    )
}

// --- reading a frame -----------------------------------------------------------------------------

fn truncated() -> Refused {
    Refused::new(
        Reason::Truncated,
        "the input ended or failed inside a frame",
    )
}

/// Exactly this many bytes, or the refusal of a frame that was cut. Nothing is set
/// aside for bytes that have not come: what is read grows as they do.
fn exactly(input: &mut impl Read, count: u64) -> Result<Vec<u8>, Refused> {
    let mut bytes = Vec::new();
    match input.take(count).read_to_end(&mut bytes) {
        Ok(_) if bytes.len() as u64 == count => Ok(bytes),
        _ => Err(truncated()),
    }
}

/// Reads one frame. `Ok(None)` is the end of the conversation: the input ended before
/// a frame's first byte. Anything else that is not a whole frame keeping every rule
/// is refused, and after a refusal the input is not read again: where the next frame
/// starts is no longer known.
///
/// `open` is what this side asked and has not had answered. A result and an event are
/// held to the request they name there, an event and a result's diagnostics are
/// counted there, and a result or an error that is read closes its request.
pub fn read(input: &mut impl Read, open: &mut Open) -> Result<Option<Message>, Refused> {
    match read_frame(input, open, false)? {
        Some(Heard::Message(message)) => Ok(Some(message)),
        // Only a side that is asked nothing hears a request by its id alone.
        Some(Heard::Asked { .. }) | None => Ok(None),
    }
}

/// Reads one frame, as [`read`] and [`hear`] say: a request is handed back by its id,
/// its payloads unread, when this side is asked nothing.
fn read_frame(
    input: &mut impl Read,
    open: &mut Open,
    asked_nothing: bool,
) -> Result<Option<Heard>, Refused> {
    let Some(stated) = first(input)? else {
        return Ok(None);
    };
    let bound = open.header_bound();
    if stated == 0 || stated > bound {
        return Err(header_too_large(stated, bound));
    }
    let header = exactly(input, stated)?;
    let mut message = parse(&header)?;
    if header_of(&message).as_bytes() != header {
        return Err(Refused::new(
            Reason::NotCanonical,
            "a header is not the canonical writing of what it holds",
        ));
    }
    held(&message, stated, open)?;
    if let (true, Message::Request { id, .. }) = (asked_nothing, &message) {
        return Ok(Some(Heard::Asked { id: id.clone() }));
    }
    let rasters: Vec<(Layout, &mut Raster)> = match &mut message {
        Message::Request {
            ask: Ask::Segment { image, .. },
            ..
        } => vec![(Layout::Rgba, image)],
        Message::Result {
            answer: Answer::Segment(found),
            ..
        } => found
            .iter_mut()
            .map(|found| (Layout::Plane, &mut found.plane))
            .collect(),
        _ => Vec::new(),
    };
    for (layout, raster) in rasters {
        let expected = layout.bytes(raster);
        let mut stated = [0_u8; 8];
        stated.copy_from_slice(&exactly(input, 8)?);
        let stated = u64::from_be_bytes(stated);
        if stated != expected {
            return Err(wrong_length(stated, expected));
        }
        raster.samples = exactly(input, expected)?;
    }
    if let Message::Result {
        answer: Answer::Segment(found),
        ..
    } = &message
    {
        // The last part of the fixed order, which only the planes show.
        let sorted = found
            .windows(2)
            .all(|pair| in_order(&pair[0], &pair[1]) != Ordering::Greater);
        if !sorted {
            return Err(out_of_order());
        }
    }
    settle(&message, open);
    Ok(Some(Heard::Message(message)))
}

/// What a side that is asked nothing reads (whizbang, of a worker, at `pack/1`): a
/// message, or a request handed back by its id alone.
#[derive(Debug, Clone, PartialEq)]
pub enum Heard {
    /// A message, read whole, as [`read`] reads one.
    Message(Message),
    /// A request: its header was read and held to every rule a header keeps, and none of
    /// its payloads was read. The input is no longer at the start of a frame, so it is
    /// not read again.
    Asked { id: String },
}

/// Reads one frame as [`read`] does, but for a request, which is refused by this side
/// before a byte of its payloads is read: it is handed back by its id, and the input
/// is not read again (ADR 0020, "The messages": a worker asks nothing of whizbang at
/// `pack/1`).
pub fn hear(input: &mut impl Read, open: &mut Open) -> Result<Option<Heard>, Refused> {
    read_frame(input, open, true)
}

/// A text of the protocol made of the words given (ADR 0020, "A text of the
/// protocol"): white space at the ends dropped, every character a text of the protocol
/// may not hold made a space, never a `?`, white space at the ends dropped again, and
/// the first `most` characters kept. It may be empty. Never the core's cleaning of a
/// fault's sentence, which puts a `?`, a character such a text may not hold.
pub fn clean(text: &str, most: usize) -> String {
    let kept = |c: char| c.is_ascii_alphanumeric() || " .,;:'()%+-_".contains(c);
    let spaced: String = text
        .trim()
        .chars()
        .map(|c| if kept(c) { c } else { ' ' })
        .collect();
    spaced.trim().chars().take(most).collect()
}

/// An error's sentence made of the words given: [`clean`] to [`FAULT_LIMIT`]
/// characters, and `no reason was given` when nothing is left.
pub fn sentence(text: &str) -> String {
    let cleaned = clean(text, FAULT_LIMIT);
    match cleaned.is_empty() {
        true => "no reason was given".to_owned(),
        false => cleaned,
    }
}

/// The length the next frame's header states; none when the input has ended.
fn first(input: &mut impl Read) -> Result<Option<u64>, Refused> {
    let mut stated = [0_u8; 4];
    let mut filled = 0;
    while filled < stated.len() {
        match input.read(&mut stated[filled..]) {
            Ok(0) if filled == 0 => return Ok(None),
            Ok(0) => return Err(truncated()),
            Ok(count) => filled += count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return Err(truncated()),
        }
    }
    Ok(Some(u64::from(u32::from_be_bytes(stated))))
}

/// A message held to what this side has open, before a payload of it is read.
fn held(message: &Message, header: u64, open: &Open) -> Result<(), Refused> {
    let unexpected = |what: &str| Refused::new(Reason::Unexpected, format!("{what} is not open"));
    match message {
        Message::Result {
            id,
            diagnostics,
            answer,
            ..
        } => {
            let awaited = open
                .awaited(id)
                .ok_or_else(|| unexpected("the request a result names"))?;
            if awaited.kind != answer.kind() {
                return Err(Refused::new(
                    Reason::Unexpected,
                    "a result is of another kind than its request",
                ));
            }
            if header > awaited.header_bound() {
                return Err(header_too_large(header, awaited.header_bound()));
            }
            let text = awaited.text + diagnostics.len() as u64;
            if text > DIAGNOSTICS_LIMIT {
                return Err(over(
                    "the text sent for one request",
                    text,
                    DIAGNOSTICS_LIMIT,
                ));
            }
            let (Answer::Segment(found), Some(allowed)) = (answer, awaited.allowed) else {
                return Ok(());
            };
            let mut counts = vec![0_u32; allowed.queries as usize];
            let mut together = 0_u64;
            for found in found {
                let Some(count) = counts.get_mut(found.query as usize) else {
                    return Err(bad("an instance's query", "one its request holds"));
                };
                *count += 1;
                if *count > allowed.max_instances {
                    let (count, limit) = (u64::from(*count), u64::from(allowed.max_instances));
                    return Err(over_instances(
                        "the count of instances for a query",
                        count,
                        limit,
                    ));
                }
                let plane = &found.plane;
                if (plane.width, plane.height) != (allowed.width, allowed.height) {
                    return Err(Refused::new(
                        Reason::WrongSize,
                        format!(
                            "a plane is {} by {}, and its request's image is {} by {}",
                            plane.width, plane.height, allowed.width, allowed.height
                        ),
                    ));
                }
                if plane.binding != allowed.binding {
                    return Err(Refused::new(
                        Reason::WrongBinding,
                        "a plane names another binding than its request's image is under",
                    ));
                }
                together += Layout::Plane.bytes(plane);
            }
            match together > RESULT_PAYLOAD_LIMIT {
                true => Err(too_large(together)),
                false => Ok(()),
            }
        }
        Message::Event { id, text } => {
            let awaited = open
                .awaited(id)
                .ok_or_else(|| unexpected("the request an event names"))?;
            if header > HEADER_LIMIT {
                return Err(header_too_large(header, HEADER_LIMIT));
            }
            if awaited.events >= EVENT_LIMIT {
                let count = u64::from(awaited.events) + 1;
                return Err(over(
                    "the count of events for one request",
                    count,
                    u64::from(EVENT_LIMIT),
                ));
            }
            let text = awaited.text + text.len() as u64;
            match text > DIAGNOSTICS_LIMIT {
                true => Err(over(
                    "the text sent for one request",
                    text,
                    DIAGNOSTICS_LIMIT,
                )),
                false => Ok(()),
            }
        }
        _ if header > HEADER_LIMIT => Err(header_too_large(header, HEADER_LIMIT)),
        _ => Ok(()),
    }
}

/// What a message that was read whole changes of what is open.
fn settle(message: &Message, open: &mut Open) {
    match message {
        Message::Event { id, text } => {
            if let Some(awaited) = open.0.get_mut(id) {
                awaited.events += 1;
                awaited.text += text.len() as u64;
            }
        }
        Message::Result { id, .. } | Message::Error { id, .. } => {
            open.0.remove(id);
        }
        _ => {}
    }
}
