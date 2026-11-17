//! A checked download and a checked probe (`SPEC.md` section 10, "Egress"; ADR 0019,
//! "A download and a probe"; plan 1B, task E3): the two ways out that carry no user
//! data, each behind the questions its kind of traffic is asked.
//!
//! A [`Downloader`] and a [`Prober`] are one request to one address, and follow
//! nothing (ADR 0012). Each of the two here is that interface around another of its
//! kind, and is the one that goes on to an address an answer points to: it asks its
//! questions of the first address and of each one after it, before anything is asked
//! of that address, and stops at the first that is refused.
//!
//! Neither is handed the bounds of a request, so each counts the answers that point
//! elsewhere within the one call it is: [`MOVES_LIMIT`] at most, as for a send. A
//! refusal is a fault in fixed words that quotes nothing of an address. It is noted
//! nowhere: neither is made for a run.

use std::fmt;
use std::sync::Arc;

use super::{
    Address, Credential, DispatchPolicy, DownloadRequest, Downloaded, Downloader, MOVES_LIMIT,
    NotAllowed, Origin, ProbeRequest, Probed, Prober, Recipient,
};
use crate::capability::{Fault, FaultKind};
use crate::contract::Cancellation;

/// The most origins a checked download is held to.
pub const DOWNLOAD_ORIGINS_LIMIT: usize = 16;

/// What a checked download answers of an origin it was not handed.
const OFF: &str = "the origin is not one this download may reach";

/// What it answers of a download that was pointed elsewhere too often.
const DOWNLOAD_MOVED: &str = "the download was pointed elsewhere more than 5 times, and is \
                              not followed further";

/// What it answers of a downloader that hands over bytes and then another address.
const BYTES_THEN_MOVED: &str = "the download handed over bytes and then pointed elsewhere, \
                                and is not followed";

const DOWNLOAD_CANCELLED: &str = "the download was cancelled";

/// What a checked probe answers of a recipient the policy does not approve for it.
const NOT_PROBED: &str = "the recipient is not approved for probing, so nothing is asked of it";

/// What it answers of a request that holds a credential.
const BROUGHT: &str = "the probe came with a credential, and whoever asks for a probe holds \
                       none";

/// What it answers of an address the recipient does not reach: the words a send is
/// refused with.
const NOT_REACHED: &str = "the origin is neither the recipient's own nor one of its upstreams";

/// What it answers of a probe that was pointed elsewhere too often.
const PROBE_MOVED: &str = "the probe was pointed elsewhere more than 5 times, and is not \
                           followed further";

const PROBE_CANCELLED: &str = "the probe was cancelled";

/// A refusal. Before anything was asked of any address it says that nothing was sent;
/// after an answer that pointed elsewhere it states no stage, since the address that
/// answered was asked.
fn refused(why: &'static str, moves: u32) -> Fault {
    let fault = Fault::new(FaultKind::Rejected, why);
    if moves == 0 { fault.not_sent() } else { fault }
}

/// The same, for a signal that was set before the next address was asked.
fn cancelled(why: &'static str, moves: u32) -> Fault {
    let fault = Fault::new(FaultKind::Cancelled, why);
    if moves == 0 { fault.not_sent() } else { fault }
}

/// A downloader held to the origins it was handed: the origins a catalogue's entry
/// names for its files. The first address of a download and every address it is
/// pointed to are asked for only at one of them.
///
/// It goes on to an address the downloader behind it was pointed to, [`MOVES_LIMIT`]
/// times at most for one download, and so never answers [`Downloaded::Moved`]. A
/// request for an address it was pointed to asks for as many bytes as the first, and
/// carries the first request's credential only when the address is at the origin
/// that credential is bound to: at every such address of the chain, whatever origin
/// the address before it was at. A download whose sink was handed a byte is not
/// followed anywhere.
///
/// What `{:?}` prints is how many origins it holds.
#[derive(Clone)]
pub struct CheckedDownloader {
    downloader: Arc<dyn Downloader>,
    origins: Vec<Origin>,
    refused: Option<Refusals>,
}

/// What is told the origin of an address a checked download refused, after the
/// refusal and before it is answered ([`CheckedDownloader::noting_refused`]).
pub type Refusals = Arc<dyn Fn(&Origin) + Send + Sync>;

impl CheckedDownloader {
    /// This downloader, held to these origins: at most [`DOWNLOAD_ORIGINS_LIMIT`]. One
    /// that is handed none reaches none.
    pub fn new(
        downloader: Arc<dyn Downloader>,
        origins: &[Origin],
    ) -> Result<CheckedDownloader, NotAllowed> {
        if origins.len() > DOWNLOAD_ORIGINS_LIMIT {
            return Err(NotAllowed::TooMany);
        }
        Ok(CheckedDownloader {
            downloader,
            origins: origins.to_vec(),
            refused: None,
        })
    }

    /// The same, telling `note` the origin, and only the origin, of each address it
    /// refuses as off its origins: what a developer's command that pins a draft entry
    /// prints, so that the host a file is moved to can be named to the owner before
    /// an entry lists it (plan 1C, task N5; open question 2). Nothing is asked of that
    /// address, and the refusal is the same fault, in the same fixed words.
    #[must_use]
    pub fn noting_refused(mut self, note: Refusals) -> CheckedDownloader {
        self.refused = Some(note);
        self
    }
}

impl fmt::Debug for CheckedDownloader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CheckedDownloader({} origins)", self.origins.len())
    }
}

impl Downloader for CheckedDownloader {
    fn download(
        &self,
        request: &DownloadRequest,
        cancellation: &Cancellation,
        sink: &mut dyn FnMut(&[u8]) -> bool,
    ) -> Result<Downloaded, Fault> {
        // The first request's credential, kept for the whole chain: a request for an
        // address at another origin carries none, and is not where the next one's
        // comes from.
        let brought = request.credential().cloned();
        let mut request = request.clone();
        let mut moves = 0;
        loop {
            if !self.origins.contains(request.from().origin()) {
                if let Some(note) = &self.refused {
                    note(request.from().origin());
                }
                return Err(refused(OFF, moves));
            }
            if cancellation.is_cancelled() {
                return Err(cancelled(DOWNLOAD_CANCELLED, moves));
            }
            let mut handed = false;
            let ended = self
                .downloader
                .download(&request, cancellation, &mut |piece| {
                    handed = true;
                    sink(piece)
                })?;
            let to = match ended {
                Downloaded::Bytes(bytes) => return Ok(Downloaded::Bytes(bytes)),
                Downloaded::Moved(_) if handed => {
                    return Err(Fault::new(FaultKind::Malformed, BYTES_THEN_MOVED));
                }
                Downloaded::Moved(to) => to,
            };
            moves += 1;
            if moves > MOVES_LIMIT {
                return Err(refused(DOWNLOAD_MOVED, moves));
            }
            request = moved_to(&request, to, brought.as_ref())?;
        }
    }
}

/// The request for the address a download was pointed to: as many bytes as the
/// request before it, which is as many as the first, and the credential the first
/// request brought only at the origin it is bound to.
fn moved_to(
    request: &DownloadRequest,
    to: Address,
    brought: Option<&Credential>,
) -> Result<DownloadRequest, Fault> {
    let malformed = |_| Fault::new(FaultKind::Malformed, "the download could not be asked for");
    let carried = brought.filter(|credential| credential.origin() == to.origin());
    let next = DownloadRequest::new(to, request.most_bytes()).map_err(malformed)?;
    match carried {
        None => Ok(next),
        Some(credential) => next.with_credential(credential.clone()).map_err(malformed),
    }
}

/// A prober for one recipient, under a policy: it probes only when the policy
/// approves probing that recipient ([`DispatchPolicy::may_probe`]), which is apart
/// from any class of data, and only at the recipient's own origin and its upstreams
/// ([`Recipient::reaches`]), the first address and every address it is pointed to.
///
/// It goes on to an address the prober behind it was pointed to, [`MOVES_LIMIT`]
/// times at most for one probe, and so never answers [`Probed::Moved`]. A request
/// that brings a credential is refused: the checked prober attaches the one it holds
/// for the origin of each address, or none. A probe has no place for anything else
/// ([`ProbeRequest`]), so it carries no user data.
///
/// What `{:?}` prints is the recipient as one is always printed and how many
/// credentials are held: never a credential.
#[derive(Clone)]
pub struct CheckedProber {
    prober: Arc<dyn Prober>,
    policy: DispatchPolicy,
    recipient: Recipient,
    /// Each bound to one origin the recipient reaches, no two to one origin.
    credentials: Vec<Credential>,
}

impl CheckedProber {
    /// This prober, for this recipient, under this policy.
    pub fn new(
        prober: Arc<dyn Prober>,
        policy: DispatchPolicy,
        recipient: Recipient,
    ) -> CheckedProber {
        CheckedProber {
            prober,
            policy,
            recipient,
            credentials: Vec::new(),
        }
    }

    /// The same, holding this credential for probes of the credential's origin.
    /// Refused: one for an origin that is neither the recipient's own nor one of its
    /// upstreams, and a second one for the same origin. So it holds one more than a
    /// recipient has upstreams, at most.
    pub fn with_credential(mut self, credential: Credential) -> Result<CheckedProber, NotAllowed> {
        if !self.recipient.reaches(credential.origin()) {
            return Err(NotAllowed::Elsewhere);
        }
        let held = |held: &Credential| held.origin() == credential.origin();
        if self.credentials.iter().any(held) {
            return Err(NotAllowed::Twice);
        }
        self.credentials.push(credential);
        Ok(self)
    }
}

impl fmt::Debug for CheckedProber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CheckedProber({:?}, {} credentials)",
            self.recipient,
            self.credentials.len()
        )
    }
}

impl Prober for CheckedProber {
    fn probe(&self, request: &ProbeRequest, cancellation: &Cancellation) -> Result<Probed, Fault> {
        if !self.policy.may_probe(&self.recipient) {
            return Err(refused(NOT_PROBED, 0));
        }
        if request.credential().is_some() {
            return Err(refused(BROUGHT, 0));
        }
        let mut to = request.to().clone();
        let mut moves = 0;
        loop {
            if !self.recipient.reaches(to.origin()) {
                return Err(refused(NOT_REACHED, moves));
            }
            if cancellation.is_cancelled() {
                return Err(cancelled(PROBE_CANCELLED, moves));
            }
            // The credential bound to this origin, and never another.
            let held = self.credentials.iter().find(|c| c.origin() == to.origin());
            let probe = match held {
                None => ProbeRequest::new(to),
                Some(credential) => ProbeRequest::new(to)
                    .with_credential(credential.clone())
                    .map_err(|_| Fault::new(FaultKind::Malformed, "the probe could not be made"))?,
            };
            match self.prober.probe(&probe, cancellation)? {
                Probed::Reached { status } => return Ok(Probed::Reached { status }),
                Probed::Moved(next) => to = next,
            }
            moves += 1;
            if moves > MOVES_LIMIT {
                return Err(refused(PROBE_MOVED, moves));
            }
        }
    }
}
