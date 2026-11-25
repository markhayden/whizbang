//! A request whose body is frames, and whose answer is read as it comes (ADR 0046, "The
//! kind `remote`": the client's way of frames; ADR 0024, as amended there): the way out
//! the kind `remote` sends `pack/1` through, beside the send of pieces ([`Outgoing`]).
//!
//! **The check sees typed parts, not opaque bytes.** A [`Frames`] holds a body of the
//! backend's own ([`FrameBody`]): what it says it is made of, each text as a piece of its
//! class and the picture as its pixels ([`Piece::pixels`]), and the bytes it writes of
//! that same value. The checked sender asks the policy and the check about the parts;
//! only then are the bytes written, once, into a [`CheckedFrames`], which only a check
//! makes ([`CheckedFrames::checked`]) and which is all a sender of frames takes. So the
//! bytes that leave are written from the value whose parts were checked, and carry the
//! classes the parts carry.
//!
//! **The answer is read as it arrives.** A sender of frames hands the answer's head and
//! a reader of its body to the caller's `take`, which reads it under its own bounds: no
//! answer is held whole by the sender.
//!
//! **A control send** ([`Control`]) is one more `POST` to the origin a request of frames
//! was already dispatched to ([`Dispatched`]), with that request's credential and no user
//! data, under bounds of its own
//! ([`Bounds::for_cleanup`](crate::capability::Bounds::for_cleanup)). It resolves no name: it
//! connects to the address the request's own connection reached.

use std::fmt;
use std::io::Read;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use super::checked::check_parts;
use super::{
    Address, Classes, Credential, DispatchPolicy, HopCheck, NotAllowed, Origin, Piece, Recipient,
};
use crate::capability::{Fault, FaultKind};

/// The most fields of its own a request of frames names.
pub const FRAME_FIELDS_LIMIT: usize = 4;

/// The most bytes of a control send's body: a hello and a `cancel` of `pack/1` are far
/// less.
pub const CONTROL_MAX_BYTES: usize = 4 * 1024;

/// What a request of frames is written from: the parts the check is shown, and the
/// bytes written of the same value. An implementation writes its bytes from the value
/// whose parts it states, and from nothing else: each text of the parts is a text of the
/// bytes, and the picture of the parts is the picture of the bytes. It is the
/// implementer's code (the kind `remote` writes `pack/1` with its own codec), and the
/// review holds it to that.
pub trait FrameBody: Send + Sync {
    /// The typed parts: each text the bytes carry as a piece of its class, and a
    /// picture as its pixels ([`Piece::pixels`]).
    fn parts(&self) -> Vec<Piece>;

    /// The bytes of the body, written from the same value.
    fn write(&self) -> Result<Vec<u8>, Fault>;
}

/// The check of a text field's name and value, as [`super::Outgoing::with_fields`]
/// holds them.
fn field_kept(name: &str, value: &str) -> bool {
    let named = (1..=64).contains(&name.len())
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
    let valued = (1..=256).contains(&value.len())
        && value.bytes().all(|b| b == b' ' || b.is_ascii_graphic())
        && !value.starts_with(' ')
        && !value.ends_with(' ');
    named && valued && !super::ways::is_senders_field(name)
}

/// A request whose body is frames, to one address: the body, the fields of its own it
/// names, and how long its answer may take, from the last byte of the request to the
/// answer's first and between two of its bytes. What `{:?}` prints is the address and
/// the fields' names.
#[derive(Clone)]
pub struct Frames {
    to: Address,
    body: Arc<dyn FrameBody>,
    fields: Vec<(&'static str, String)>,
    answer_within: Duration,
}

impl Frames {
    /// This body, for this address, its answer waited for at most `answer_within`
    /// between two of its bytes (and never past the request's deadline).
    pub fn new(to: Address, body: Arc<dyn FrameBody>, answer_within: Duration) -> Frames {
        Frames {
            to,
            body,
            fields: Vec::new(),
            answer_within,
        }
    }

    /// The same, naming this field of its own. Refused: more than
    /// [`FRAME_FIELDS_LIMIT`], a name or a value that is not one
    /// ([`super::Outgoing::with_fields`]'s rules: one the sender writes itself or one
    /// that carries a credential is refused), and a name given twice.
    pub fn with_field(mut self, name: &'static str, value: &str) -> Result<Frames, NotAllowed> {
        if self.fields.len() >= FRAME_FIELDS_LIMIT {
            return Err(NotAllowed::TooMany);
        }
        if !field_kept(name, value) {
            return Err(NotAllowed::Name);
        }
        if self
            .fields
            .iter()
            .any(|(held, _)| held.eq_ignore_ascii_case(name))
        {
            return Err(NotAllowed::Twice);
        }
        self.fields.push((name, value.to_owned()));
        Ok(self)
    }

    pub fn to(&self) -> &Address {
        &self.to
    }

    /// The parts of the body, as the check is shown them.
    pub fn parts(&self) -> Vec<Piece> {
        self.body.parts()
    }

    pub fn fields(&self) -> &[(&'static str, String)] {
        &self.fields
    }

    pub fn answer_within(&self) -> Duration {
        self.answer_within
    }
}

impl fmt::Debug for Frames {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&str> = self.fields.iter().map(|(name, _)| *name).collect();
        write!(f, "Frames({:?}, fields {names:?})", self.to)
    }
}

/// A request of frames that a check let through, its bytes written: what a sender of
/// frames sends, and all it sends. Made by [`CheckedFrames::checked`] alone. What
/// `{:?}` prints is the address, the classes and the size, never a byte.
pub struct CheckedFrames {
    to: Address,
    bytes: Vec<u8>,
    needs: Classes,
    fields: Vec<(&'static str, String)>,
    credential: Option<Credential>,
    answer_within: Duration,
}

impl CheckedFrames {
    /// Asks `check` about the parts of `frames` as they would leave to `recipient`,
    /// under `policy`: the address is held to the recipient's origin and upstreams, the
    /// policy is asked about every class the parts hold, and the check is shown the
    /// parts and the fields, as a send's are ([`HopCheck`]). Only then are the bytes
    /// written, from the body whose parts were checked. `credential`, bound to the
    /// address's origin, is carried; one bound elsewhere is refused.
    ///
    /// Each refusal says why in fixed words, and that nothing was sent.
    pub fn checked(
        frames: &Frames,
        recipient: &Recipient,
        policy: &DispatchPolicy,
        check: &dyn HopCheck,
        credential: Option<Credential>,
    ) -> Result<CheckedFrames, Fault> {
        let parts = frames.parts();
        let needs = Classes::needed_for(parts.iter().map(Piece::class));
        let fields: Vec<(&str, &str)> = frames
            .fields
            .iter()
            .map(|(name, value)| (*name, value.as_str()))
            .collect();
        check_parts(
            recipient, &frames.to, &parts, &fields, needs, policy, check, 0,
        )
        .map_err(|why| Fault::new(FaultKind::Rejected, why).not_sent())?;
        CheckedFrames::written(frames, needs, credential)
    }

    /// The bytes of `frames` written, once its parts were checked: what the checked
    /// sender makes, past its own questions. `credential` is carried only when it is
    /// bound to the address's origin.
    pub(super) fn written(
        frames: &Frames,
        needs: Classes,
        credential: Option<Credential>,
    ) -> Result<CheckedFrames, Fault> {
        let credential = match credential {
            None => None,
            Some(credential) if credential.origin() == frames.to.origin() => Some(credential),
            Some(_) => {
                return Err(Fault::new(
                    FaultKind::Rejected,
                    "the credential is bound to another origin than the request's",
                )
                .not_sent());
            }
        };
        let bytes = frames.body.write()?;
        Ok(CheckedFrames {
            to: frames.to.clone(),
            bytes,
            needs,
            fields: frames.fields.clone(),
            credential,
            answer_within: frames.answer_within,
        })
    }

    pub fn to(&self) -> &Address {
        &self.to
    }

    /// The bytes of the body.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The classes of data the parts held.
    pub fn needs(&self) -> Classes {
        self.needs
    }

    pub fn fields(&self) -> &[(&'static str, String)] {
        &self.fields
    }

    pub fn credential(&self) -> Option<&Credential> {
        self.credential.as_ref()
    }

    pub fn answer_within(&self) -> Duration {
        self.answer_within
    }
}

impl fmt::Debug for CheckedFrames {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CheckedFrames({:?}, {:?}, {} bytes)",
            self.to,
            self.needs,
            self.bytes.len()
        )
    }
}

/// The head of an answer to a request of frames: its status, and its fields as they
/// came, in order. What `{:?}` prints is the status and the count of fields.
#[derive(Clone, PartialEq, Eq)]
pub struct Head {
    status: u16,
    fields: Vec<(String, String)>,
}

impl Head {
    /// An answer's head: what a sender read.
    pub fn new(status: u16, fields: Vec<(String, String)>) -> Head {
        Head { status, fields }
    }

    pub fn status(&self) -> u16 {
        self.status
    }

    /// The one value of the field of this name, compared without regard to case: none
    /// when the answer has none, and none when it has more than one, which is no value
    /// a reader can take.
    pub fn field(&self, name: &str) -> Option<&str> {
        let mut all = self
            .fields
            .iter()
            .filter(|(field, _)| field.eq_ignore_ascii_case(name));
        let first = all.next()?;
        match all.next() {
            Some(_) => None,
            None => Some(first.1.as_str()),
        }
    }

    /// Whether the answer has a field of this name, once or more.
    pub fn has(&self, name: &str) -> bool {
        self.fields
            .iter()
            .any(|(field, _)| field.eq_ignore_ascii_case(name))
    }
}

impl fmt::Debug for Head {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Head(status {}, {} fields)",
            self.status,
            self.fields.len()
        )
    }
}

/// What the caller of a request of frames is handed of its answer: the head, and a
/// reader of the body as it arrives. What it answers is the request's end, unless the
/// connection failed under it first, which the sender then answers instead.
pub type Take<'a> = dyn FnMut(&Head, &mut dyn Read) -> Result<(), Fault> + 'a;

/// A request of frames that reached its host: the origin, the address its connection
/// reached, and the mark of the checked sender that let it through, by which a control
/// send to it is allowed ([`Control`]). What `{:?}` prints is the origin.
#[derive(Clone, PartialEq, Eq)]
pub struct Dispatched {
    origin: Origin,
    peer: SocketAddr,
    mark: u64,
}

impl Dispatched {
    /// A request to this origin whose connection reached this address: what a sender of
    /// frames says once it has connected.
    pub fn reached(origin: Origin, peer: SocketAddr) -> Dispatched {
        Dispatched {
            origin,
            peer,
            mark: 0,
        }
    }

    pub(crate) fn marked(mut self, mark: u64) -> Dispatched {
        self.mark = mark;
        self
    }

    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    /// The address the request's connection reached: a control send connects to it,
    /// and resolves no name.
    pub fn peer(&self) -> SocketAddr {
        self.peer
    }
}

impl fmt::Debug for Dispatched {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Dispatched({})", self.origin)
    }
}

/// How a request of frames ended: the request it reached, when it reached one, whether
/// any byte of the request left, and its end. A request that failed after it connected
/// still names what it reached, so that a control send can follow it. A connection that
/// opened and failed before the first byte of the request was written to it (in its
/// handshake, say) reached a host that was handed nothing: `left` is false, and its fault
/// is [`Stage::NotSent`](crate::capability::Stage::NotSent).
#[derive(Debug)]
pub struct Sent {
    pub dispatched: Option<Dispatched>,
    pub left: bool,
    pub ended: Result<(), Fault>,
}

impl Sent {
    /// A request refused before anything left.
    pub fn refused(fault: Fault) -> Sent {
        Sent {
            dispatched: None,
            left: false,
            ended: Err(fault),
        }
    }
}

/// A control send: a `POST` of a few bytes that carry no user data (a cancel of
/// `pack/1`), to an address at the origin of a request already dispatched. What
/// `{:?}` prints is the address and the size.
#[derive(Clone)]
pub struct Control {
    to: Address,
    body: Vec<u8>,
    fields: Vec<(&'static str, String)>,
    credential: Option<Credential>,
}

impl Control {
    /// These bytes, for this address: at most [`CONTROL_MAX_BYTES`].
    pub fn new(to: Address, body: Vec<u8>) -> Result<Control, NotAllowed> {
        if body.len() > CONTROL_MAX_BYTES {
            return Err(NotAllowed::TooLong);
        }
        Ok(Control {
            to,
            body,
            fields: Vec::new(),
            credential: None,
        })
    }

    /// The same, naming this field of its own, under [`Frames::with_field`]'s rules.
    pub fn with_field(mut self, name: &'static str, value: &str) -> Result<Control, NotAllowed> {
        if self.fields.len() >= FRAME_FIELDS_LIMIT {
            return Err(NotAllowed::TooMany);
        }
        if !field_kept(name, value) {
            return Err(NotAllowed::Name);
        }
        if self
            .fields
            .iter()
            .any(|(held, _)| held.eq_ignore_ascii_case(name))
        {
            return Err(NotAllowed::Twice);
        }
        self.fields.push((name, value.to_owned()));
        Ok(self)
    }

    /// The same, carrying this credential: what the checked sender attaches, the one of
    /// the request it follows. The checked sender refuses a control that a backend
    /// hands it carrying one: a backend holds none.
    pub fn carrying(mut self, credential: Option<Credential>) -> Control {
        self.credential = credential;
        self
    }

    pub fn to(&self) -> &Address {
        &self.to
    }

    pub fn body(&self) -> &[u8] {
        &self.body
    }

    pub fn fields(&self) -> &[(&'static str, String)] {
        &self.fields
    }

    pub fn credential(&self) -> Option<&Credential> {
        self.credential.as_ref()
    }
}

impl fmt::Debug for Control {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Control({:?}, {} bytes)", self.to, self.body.len())
    }
}

/// How long a control send has, from its connect to its last byte: the bounds it is
/// made under ([`Bounds::for_cleanup`](crate::capability::Bounds::for_cleanup)).
pub const CONTROL_WITHIN: Duration = Duration::from_secs(5);
