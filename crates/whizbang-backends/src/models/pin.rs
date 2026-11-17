//! Pinning a draft entry (plan 1C, task N5): each file of an entry whose sizes and
//! hashes are not known yet is fetched once, through the downloader it is handed, and
//! its size and SHA-256 are worked out from the bytes that came. Nothing is written:
//! the bytes are hashed as they come and dropped. It is how a catalogue entry's small
//! files are given their sizes and hashes, and it is trust on first use: what is
//! pinned is what the origin served that once.
//!
//! A draft's size for a file is the most bytes that are wanted of it, not its size:
//! one byte more is asked for, and a file that has it is said to be over its bound,
//! and is not hashed.

use sha2::{Digest as _, Sha256};
use whizbang_core::capability::Fault;
use whizbang_core::contract::Cancellation;
use whizbang_core::egress::{DownloadRequest, Downloaded};
use whizbang_core::identity::Digest;

use super::install::Carrying;
use super::stage::{MOVES_LIMIT, attached};
use super::{CatalogueFile, Entry};

/// Why one file of a draft was not pinned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinRefused {
    /// More bytes came than the draft's bound for the file.
    OverBound,
    /// The file moved to an address at an origin the draft does not name. Nothing was
    /// asked of that address.
    Elsewhere,
    /// The file moved more than [`MOVES_LIMIT`] times.
    TooManyMoves,
    /// The request was cancelled.
    Cancelled,
    /// The download failed with this fault.
    Download(Fault),
}

/// What one file was pinned as: its size and the SHA-256 of its bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pinned {
    pub size: u64,
    pub sha256: Digest,
}

/// Fetches this file of this draft entry once, through what `carrying` gives, and
/// answers its size and hash; nothing is written. A request for an address at the
/// origin the credential is bound to carries it, and one at any other origin carries
/// none, as an install's do. An address the file is moved to is asked for only when
/// it is at one of the entry's origins.
pub fn pin_file(
    entry: &Entry,
    file: &CatalogueFile,
    carrying: &Carrying<'_>,
    cancellation: &Cancellation,
) -> Result<Pinned, PinRefused> {
    let bound = file.size();
    let (mut kept, mut beyond, mut hash) = (0u64, false, Sha256::new());
    let mut from = file.from().clone();
    let mut moves = 0;
    loop {
        let request = DownloadRequest::new(from, bound + 1)
            .and_then(|request| attached(request, carrying.credential))
            .map_err(|_| {
                PinRefused::Download(Fault::new(
                    whizbang_core::capability::FaultKind::Malformed,
                    "the download could not be asked for",
                ))
            })?;
        let ended = carrying
            .downloader
            .download(&request, cancellation, &mut |piece| {
                if beyond || kept.saturating_add(piece.len() as u64) > bound {
                    beyond = true;
                    return false;
                }
                hash.update(piece);
                kept += piece.len() as u64;
                true
            });
        if cancellation.is_cancelled() {
            return Err(PinRefused::Cancelled);
        }
        match ended {
            Err(fault) => return Err(PinRefused::Download(fault)),
            Ok(Downloaded::Bytes(_)) => break,
            Ok(Downloaded::Moved(_)) if kept > 0 || beyond => {
                return Err(PinRefused::Download(Fault::new(
                    whizbang_core::capability::FaultKind::Malformed,
                    "the download handed over bytes and then pointed elsewhere",
                )));
            }
            Ok(Downloaded::Moved(to)) => {
                moves += 1;
                if moves > MOVES_LIMIT {
                    return Err(PinRefused::TooManyMoves);
                }
                if !entry.may_download_from(to.origin()) {
                    return Err(PinRefused::Elsewhere);
                }
                from = to;
            }
        }
    }
    if beyond {
        return Err(PinRefused::OverBound);
    }
    Ok(Pinned {
        size: kept,
        sha256: Digest::from_bytes(hash.finalize().into()),
    })
}
