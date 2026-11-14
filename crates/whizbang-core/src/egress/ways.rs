//! The three interfaces through which anything leaves the machine (`SPEC.md` section
//! 10, "Egress"; ADR 0012): one for data, one for a model's file, one for a probe.
//!
//! Each is one request to one address, answered once. None follows a redirect: an
//! answer that points elsewhere is handed back as the address it points to, so that
//! whoever drives the hops can check each before anything is handed to it. The core
//! holds no implementation, and nothing here decides where a request may go.

use std::fmt;

use super::{Address, Classes, Credential, DataClass, NotAllowed};
use crate::capability::{Bounds, Fault};
use crate::contract::Cancellation;

/// The most pieces one request holds.
pub const PIECES_LIMIT: usize = 64;

/// The most bytes of a reply's body.
pub const REPLY_MAX_BYTES: usize = 16 << 20;

/// One part of what is sent: its bytes, and the class of data they are, when whoever
/// put the part in stated one. What `{:?}` prints is the class and the size, never a
/// byte.
#[derive(Clone, PartialEq, Eq)]
pub struct Piece {
    class: Option<DataClass>,
    bytes: Vec<u8>,
}

impl Piece {
    /// A piece of this class.
    pub fn of(class: DataClass, bytes: Vec<u8>) -> Piece {
        Piece {
            class: Some(class),
            bytes,
        }
    }

    /// A piece whose class nobody stated. A recipient needs every class approved to be
    /// handed it.
    pub fn unclassed(bytes: Vec<u8>) -> Piece {
        Piece { class: None, bytes }
    }

    /// The class that was stated, when one was.
    pub fn class(&self) -> Option<DataClass> {
        self.class
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl fmt::Debug for Piece {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let class = self.class.map_or("no class", DataClass::name);
        write!(f, "Piece({class}, {} bytes)", self.bytes.len())
    }
}

/// What is sent to one address: pieces of data, and a credential when the address's
/// origin is the one the credential is bound to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outgoing {
    to: Address,
    pieces: Vec<Piece>,
    credential: Option<Credential>,
}

impl Outgoing {
    /// These pieces, for this address: at most [`PIECES_LIMIT`] of them.
    pub fn new(to: Address, pieces: Vec<Piece>) -> Result<Outgoing, NotAllowed> {
        if pieces.len() > PIECES_LIMIT {
            return Err(NotAllowed::TooMany);
        }
        Ok(Outgoing {
            to,
            pieces,
            credential: None,
        })
    }

    /// The same, carrying this credential. Refused when the credential is bound to
    /// another origin than the address's: a credential is never forwarded.
    pub fn with_credential(mut self, credential: Credential) -> Result<Outgoing, NotAllowed> {
        self.credential = Some(credential.bound_to(&self.to)?);
        Ok(self)
    }

    pub fn to(&self) -> &Address {
        &self.to
    }

    pub fn pieces(&self) -> &[Piece] {
        &self.pieces
    }

    pub fn credential(&self) -> Option<&Credential> {
        self.credential.as_ref()
    }

    /// The classes a recipient must be approved for before it is handed this: the
    /// class of each piece, and all four when any piece states none
    /// ([`Classes::needed_for`]).
    pub fn needs(&self) -> Classes {
        Classes::needed_for(self.pieces.iter().map(Piece::class))
    }
}

/// What came back from one address: a status and a body, or the address the request
/// was moved to. What `{:?}` prints is the status and the body's size, or where it was
/// moved to.
#[derive(Clone, PartialEq, Eq)]
pub struct Reply {
    answer: Answer,
}

#[derive(Clone, PartialEq, Eq)]
enum Answer {
    Answered { status: u16, body: Vec<u8> },
    Moved(Address),
}

impl Reply {
    /// An answer: its status, and a body of at most [`REPLY_MAX_BYTES`]. The bound is
    /// checked on bytes that are already in memory; whoever reads an answer off a wire
    /// bounds what it reads.
    pub fn answered(status: u16, body: Vec<u8>) -> Result<Reply, NotAllowed> {
        if body.len() > REPLY_MAX_BYTES {
            return Err(NotAllowed::TooLong);
        }
        Ok(Reply {
            answer: Answer::Answered { status, body },
        })
    }

    /// The request was not answered here: it was pointed to this address. Nothing has
    /// been sent there.
    pub fn moved(to: Address) -> Reply {
        Reply {
            answer: Answer::Moved(to),
        }
    }

    /// The status, when the request was answered.
    pub fn status(&self) -> Option<u16> {
        match &self.answer {
            Answer::Answered { status, .. } => Some(*status),
            Answer::Moved(_) => None,
        }
    }

    /// The body, when the request was answered; nothing otherwise.
    pub fn body(&self) -> &[u8] {
        match &self.answer {
            Answer::Answered { body, .. } => body,
            Answer::Moved(_) => &[],
        }
    }

    /// Where the request was pointed to, when it was not answered.
    pub fn moved_to(&self) -> Option<&Address> {
        match &self.answer {
            Answer::Answered { .. } => None,
            Answer::Moved(to) => Some(to),
        }
    }
}

impl fmt::Debug for Reply {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.answer {
            Answer::Answered { status, body } => {
                write!(f, "Reply(status {status}, {} bytes)", body.len())
            }
            Answer::Moved(to) => write!(f, "Reply(moved to {to:?})"),
        }
    }
}

/// The way data leaves: one request to one address. It is handed the bounds of the
/// backend's request it is part of, and is expected to stop early when the run is
/// cancelled. It does not follow a redirect.
pub trait Sender: Send + Sync {
    fn send(&self, outgoing: &Outgoing, bounds: &Bounds) -> Result<Reply, Fault>;
}

/// What a download asks for: an address, the most bytes wanted, and a credential bound
/// to the address's origin. It has no place for a body, a picture or a prompt: a
/// download carries no user data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadRequest {
    from: Address,
    most_bytes: u64,
    credential: Option<Credential>,
}

impl DownloadRequest {
    /// A download from this address of at most this many bytes, which is one or more.
    pub fn new(from: Address, most_bytes: u64) -> Result<DownloadRequest, NotAllowed> {
        if most_bytes == 0 {
            return Err(NotAllowed::Empty);
        }
        Ok(DownloadRequest {
            from,
            most_bytes,
            credential: None,
        })
    }

    /// The same, carrying this credential. Refused when the credential is bound to
    /// another origin than the address's.
    pub fn with_credential(
        mut self,
        credential: Credential,
    ) -> Result<DownloadRequest, NotAllowed> {
        self.credential = Some(credential.bound_to(&self.from)?);
        Ok(self)
    }

    pub fn from(&self) -> &Address {
        &self.from
    }

    /// The most bytes wanted. Whoever downloads is expected to hand over no more; the
    /// caller's sink is what holds it to that.
    pub fn most_bytes(&self) -> u64 {
        self.most_bytes
    }

    pub fn credential(&self) -> Option<&Credential> {
        self.credential.as_ref()
    }
}

/// How one request for a file ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Downloaded {
    /// This many bytes were handed to the sink, and the sink took them.
    Bytes(u64),
    /// The file is not here: the request was pointed to this address. Nothing has been
    /// asked of it.
    Moved(Address),
}

/// The way a model's file arrives: one request to one address. The bytes are handed to
/// `sink` piece by piece, in order; the sink answers whether to go on, and a download
/// stops when it says no. It is bounded by a cancellation signal, and does not follow
/// a redirect.
pub trait Downloader: Send + Sync {
    fn download(
        &self,
        request: &DownloadRequest,
        cancellation: &Cancellation,
        sink: &mut dyn FnMut(&[u8]) -> bool,
    ) -> Result<Downloaded, Fault>;
}

/// What a probe asks for: an address, and a credential bound to the address's origin.
/// It has no place for anything else: a probe carries no user data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeRequest {
    to: Address,
    credential: Option<Credential>,
}

impl ProbeRequest {
    /// A probe of this address.
    pub fn new(to: Address) -> ProbeRequest {
        ProbeRequest {
            to,
            credential: None,
        }
    }

    /// The same, carrying this credential. Refused when the credential is bound to
    /// another origin than the address's.
    pub fn with_credential(mut self, credential: Credential) -> Result<ProbeRequest, NotAllowed> {
        self.credential = Some(credential.bound_to(&self.to)?);
        Ok(self)
    }

    pub fn to(&self) -> &Address {
        &self.to
    }

    pub fn credential(&self) -> Option<&Credential> {
        self.credential.as_ref()
    }
}

/// How one probe ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probed {
    /// The address answered, with this status.
    Reached { status: u16 },
    /// The probe was pointed to this address. Nothing has been asked of it.
    Moved(Address),
}

/// The way a backend is probed: one request to one address, bounded by a cancellation
/// signal. It does not follow a redirect.
pub trait Prober: Send + Sync {
    fn probe(&self, request: &ProbeRequest, cancellation: &Cancellation) -> Result<Probed, Fault>;
}

#[cfg(test)]
mod tests {
    //! The fields of the two requests that carry no user data, all of them. A pattern
    //! that names every field stops compiling when one is added, which a test outside
    //! the crate cannot do with private fields.

    use super::*;

    #[test]
    fn a_download_and_a_probe_have_these_fields_and_no_other() {
        let no_secrets = crate::contract::Secrets::none();
        let Ok(address) = Address::of("https://weights.example.org/m", &no_secrets) else {
            panic!("the address is one");
        };
        let Ok(request) = DownloadRequest::new(address.clone(), 7) else {
            panic!("the request is one");
        };
        let DownloadRequest {
            from,
            most_bytes,
            credential,
        } = request;
        let (_, _, _): (Address, u64, Option<Credential>) = (from, most_bytes, credential);
        let ProbeRequest { to, credential } = ProbeRequest::new(address);
        let (_, _): (Address, Option<Credential>) = (to, credential);
    }
}
