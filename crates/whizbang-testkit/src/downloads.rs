//! A fake of the interface through which a model's file is downloaded (ADR 0012,
//! `Downloader`): it fetches nothing from anywhere. It serves bytes from memory, a few
//! kilobytes, in pieces, and can be told to answer that a file moved, to fail at once
//! or after a stated count of bytes, to stop at a barrier after a stated count of
//! bytes, and to hand over more than it was asked for. It keeps every request, in the
//! order they came, with how many bytes it handed over for each.
//!
//! Wrong bytes and a short file need no telling: a test serves the bytes it wants
//! served.

use std::sync::{Mutex, MutexGuard, PoisonError};

use whizbang_core::capability::{Fault, FaultKind};
use whizbang_core::contract::Cancellation;
use whizbang_core::egress::{Address, DownloadRequest, Downloaded, Downloader};

use crate::barrier::Barriers;

/// The size of a piece, unless the fake is told another.
pub const PIECE_BYTES: usize = 1024;

/// How one request to the fake ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    /// It has not ended: the fake is handing pieces over, or is stopped at a barrier.
    Open,
    /// Everything the fake had for it, or the most it asked for, was handed over.
    Whole,
    /// The sink answered no.
    Stopped,
    /// It was answered with the address the file moved to.
    Moved,
    /// It ended in a fault: a scripted one, a cancellation, or an address the fake
    /// serves nothing at.
    Failed,
}

/// One request the fake was handed. What `{:?}` prints is the address's origin, the
/// numbers and how it ended: no credential, and nothing of the address's rest.
#[derive(Clone, PartialEq, Eq)]
pub struct Asked {
    request: DownloadRequest,
    handed: u64,
    ended: Ended,
}

impl Asked {
    /// The address the request was for.
    pub fn from(&self) -> &Address {
        self.request.from()
    }

    /// The most bytes it asked for.
    pub fn most_bytes(&self) -> u64 {
        self.request.most_bytes()
    }

    /// How many bytes were handed to its sink, so far.
    pub fn handed(&self) -> u64 {
        self.handed
    }

    /// How it ended, or that it has not.
    pub fn ended(&self) -> Ended {
        self.ended
    }

    /// The headers the request would have carried, each a name and a value: the
    /// credential's, when the request holds one, and no other.
    pub fn headers(&self) -> Vec<(String, String)> {
        self.request
            .credential()
            .map(|credential| {
                (
                    credential.header().to_owned(),
                    credential.expose().to_owned(),
                )
            })
            .into_iter()
            .collect()
    }
}

impl std::fmt::Debug for Asked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Asked({}, most {}, handed {}, {:?})",
            self.request.from().origin(),
            self.request.most_bytes(),
            self.handed,
            self.ended
        )
    }
}

/// What the fake does with a request for one address.
#[derive(Clone)]
enum Served {
    /// The file's bytes, and what happens on the way.
    File {
        bytes: Vec<u8>,
        /// Whether the most a request asks for is looked at.
        bounded: bool,
        /// What happens once this many bytes were handed over, when anything does.
        then: Option<(u64, Then)>,
    },
    Moved(Address),
    Fault(Fault),
}

#[derive(Clone)]
enum Then {
    Fail(Fault),
    Stop(Barriers, String),
}

/// A fake downloader. It may be shared across threads. It holds no lock while it
/// hands a piece to a sink or is stopped at a barrier.
///
/// An address it was told nothing of is answered with the fault `unavailable`, never
/// with a panic. Told twice of one address, the later telling stands.
pub struct FakeDownloader {
    piece: usize,
    served: Vec<(Address, Served)>,
    asked: Mutex<Vec<Asked>>,
}

impl Default for FakeDownloader {
    fn default() -> FakeDownloader {
        FakeDownloader::new()
    }
}

impl FakeDownloader {
    /// A downloader that serves nothing yet, in pieces of [`PIECE_BYTES`].
    pub fn new() -> FakeDownloader {
        FakeDownloader {
            piece: PIECE_BYTES,
            served: Vec::new(),
            asked: Mutex::new(Vec::new()),
        }
    }

    /// The same, handing over pieces of this many bytes: one at least.
    pub fn in_pieces_of(mut self, bytes: usize) -> FakeDownloader {
        self.piece = bytes.max(1);
        self
    }

    fn tell(mut self, at: &Address, served: Served) -> FakeDownloader {
        self.served.retain(|(address, _)| address != at);
        self.served.push((at.clone(), served));
        self
    }

    fn file(self, at: &Address, bytes: Vec<u8>, then: Option<(u64, Then)>) -> FakeDownloader {
        self.tell(
            at,
            Served::File {
                bytes,
                bounded: true,
                then,
            },
        )
    }

    /// Serves these bytes at this address, and no more of them than a request asks
    /// for.
    pub fn serving(self, at: &Address, bytes: Vec<u8>) -> FakeDownloader {
        self.file(at, bytes, None)
    }

    /// Serves these bytes at this address, all of them, whatever the most a request
    /// asks for: a downloader that does not keep to what it was asked. Only the sink
    /// stops it.
    pub fn serving_too_many(self, at: &Address, bytes: Vec<u8>) -> FakeDownloader {
        self.tell(
            at,
            Served::File {
                bytes,
                bounded: false,
                then: None,
            },
        )
    }

    /// Answers a request for this address with the address the file moved to. The
    /// fake asks nothing of that address: it follows nothing.
    pub fn moved(self, at: &Address, to: Address) -> FakeDownloader {
        self.tell(at, Served::Moved(to))
    }

    /// Fails every request for this address with this fault, before a byte.
    pub fn failing(self, at: &Address, fault: Fault) -> FakeDownloader {
        self.tell(at, Served::Fault(fault))
    }

    /// Serves these bytes at this address until exactly `after` of them were handed
    /// over, and then fails with this fault. A request that ends before that many, or
    /// exactly there, is answered: the fault is for one that would have gone on.
    pub fn failing_after(
        self,
        at: &Address,
        bytes: Vec<u8>,
        after: u64,
        fault: Fault,
    ) -> FakeDownloader {
        self.file(at, bytes, Some((after, Then::Fail(fault))))
    }

    /// Serves these bytes at this address, and once exactly `after` of them were
    /// handed over stops at the barrier `name` until a test releases it. A request
    /// cancelled by then ends there as cancelled; one that ends before that many, or
    /// exactly there, does not stop.
    pub fn stopping_after(
        self,
        at: &Address,
        bytes: Vec<u8>,
        after: u64,
        barriers: Barriers,
        name: &str,
    ) -> FakeDownloader {
        self.file(
            at,
            bytes,
            Some((after, Then::Stop(barriers, name.to_owned()))),
        )
    }

    fn kept(&self) -> MutexGuard<'_, Vec<Asked>> {
        self.asked.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Every request it was handed, in the order they came, as each stands now.
    pub fn requests(&self) -> Vec<Asked> {
        self.kept().clone()
    }

    /// How many requests it was handed for this address, however they ended.
    pub fn asked(&self, at: &Address) -> u64 {
        self.kept()
            .iter()
            .filter(|asked| asked.from() == at)
            .count() as u64
    }

    /// How many bytes it handed over for this address, over all its requests.
    pub fn handed(&self, at: &Address) -> u64 {
        self.kept()
            .iter()
            .filter(|asked| asked.from() == at)
            .map(Asked::handed)
            .sum()
    }

    /// Hands the bytes over in pieces, counting each piece on the request numbered
    /// `at` once the sink has seen it.
    fn hand_over(
        &self,
        at: usize,
        bytes: &[u8],
        then: Option<&(u64, Then)>,
        cancellation: &Cancellation,
        sink: &mut dyn FnMut(&[u8]) -> bool,
    ) -> Result<Ended, Fault> {
        let cancelled = || Fault::new(FaultKind::Cancelled, "the download was cancelled");
        // Where what was scripted happens, when the request gets that far.
        let turn = then
            .and_then(|(after, _)| usize::try_from(*after).ok())
            .filter(|after| *after < bytes.len());
        let mut done = 0;
        loop {
            if cancellation.is_cancelled() {
                return Err(cancelled());
            }
            if let (Some(turn), Some((_, then))) = (turn, then)
                && done == turn
            {
                match then {
                    Then::Fail(fault) => return Err(fault.clone()),
                    Then::Stop(barriers, name) => {
                        barriers.stop_at(name);
                        if cancellation.is_cancelled() {
                            return Err(cancelled());
                        }
                    }
                }
            }
            if done == bytes.len() {
                return Ok(Ended::Whole);
            }
            // A piece ends where what was scripted happens, so that exactly that many
            // bytes were handed over when it does.
            let end = match turn {
                Some(turn) if done < turn => turn,
                _ => bytes.len(),
            };
            let piece = &bytes[done..end.min(done + self.piece)];
            let go_on = sink(piece);
            done += piece.len();
            self.kept()[at].handed += piece.len() as u64;
            if !go_on {
                return Ok(Ended::Stopped);
            }
        }
    }
}

impl std::fmt::Debug for FakeDownloader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "FakeDownloader({} addresses, {} requests)",
            self.served.len(),
            self.kept().len()
        )
    }
}

impl Downloader for FakeDownloader {
    fn download(
        &self,
        request: &DownloadRequest,
        cancellation: &Cancellation,
        sink: &mut dyn FnMut(&[u8]) -> bool,
    ) -> Result<Downloaded, Fault> {
        let at = {
            let mut kept = self.kept();
            kept.push(Asked {
                request: request.clone(),
                handed: 0,
                ended: Ended::Open,
            });
            kept.len() - 1
        };
        let served = self
            .served
            .iter()
            .find(|(address, _)| address == request.from())
            .map(|(_, served)| served);
        let outcome = match served {
            None => Err(Fault::new(
                FaultKind::Unavailable,
                "the fake serves nothing at that address",
            )),
            Some(Served::Fault(fault)) => Err(fault.clone()),
            Some(Served::Moved(to)) => Ok((Ended::Moved, Some(to.clone()))),
            Some(Served::File {
                bytes,
                bounded,
                then,
            }) => {
                let most = usize::try_from(request.most_bytes()).unwrap_or(usize::MAX);
                let bytes = if *bounded && most < bytes.len() {
                    &bytes[..most]
                } else {
                    &bytes[..]
                };
                self.hand_over(at, bytes, then.as_ref(), cancellation, sink)
                    .map(|ended| (ended, None))
            }
        };
        let mut kept = self.kept();
        match outcome {
            Err(fault) => {
                kept[at].ended = Ended::Failed;
                Err(fault)
            }
            Ok((ended, moved)) => {
                kept[at].ended = ended;
                Ok(match moved {
                    Some(to) => Downloaded::Moved(to),
                    None => Downloaded::Bytes(kept[at].handed),
                })
            }
        }
    }
}
