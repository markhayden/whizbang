//! What is decoded, kept while a run asks for it (plan mE, task L2, item 4).
//!
//! An image is decoded whenever it is read: an artifact read from a store is identified,
//! and [`crate::artifact::Image::pixels`] decodes its file again. A located retry of the
//! loop `remove_checked` reads its picture in every `view` and every `carry` it calls, so
//! eight places decoded the picture sixteen times. While a [`KeptDecodes`] is held, every
//! file identified is kept, decoded, and one identified again under the same limits is
//! the kept copy: the loop holds one for its run, so its picture is decoded once.
//!
//! What is kept is bounded: the [`KEPT`] files identified last, the oldest let go first,
//! and all of it when the last [`KeptDecodes`] is let go. At most four pictures of the
//! decode limits' 50 megapixels, at four bytes a pixel, is about 800 MB while one is held.
//! A copy is shared, never copied again ([`super::identify_shared`]). Nothing is kept while none is
//! held, and a file is then not even hashed for it. Every run of the process shares what
//! is kept while one is held; a copy is the same identification as a decode, since a
//! decode of the same bytes under the same limits gives the same pixels.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, PoisonError};

use super::Result;
use super::digest::Digest;
use super::pixels::{DecodeLimits, Identified};

/// The most files kept at once: the picture, and the crops and fills read beside it.
pub const KEPT: usize = 4;

/// How many decodes are counted, by file, at most: the oldest are let go first.
const COUNTED: usize = 64;

#[derive(Default)]
struct Kept {
    held: usize,
    files: VecDeque<(Digest, DecodeLimits, Arc<Identified>)>,
    decodes: VecDeque<(Digest, u64)>,
}

static KEPT_FILES: Mutex<Kept> = Mutex::new(Kept {
    held: 0,
    files: VecDeque::new(),
    decodes: VecDeque::new(),
});

fn kept() -> std::sync::MutexGuard<'static, Kept> {
    KEPT_FILES.lock().unwrap_or_else(PoisonError::into_inner)
}

/// While held, what is decoded is kept ([`keep_decoded`]).
#[must_use = "what is decoded is kept only while this is held"]
pub struct KeptDecodes(());

/// Keeps what is decoded until the guard, and every other one, is let go.
pub fn keep_decoded() -> KeptDecodes {
    kept().held += 1;
    KeptDecodes(())
}

impl Drop for KeptDecodes {
    fn drop(&mut self) {
        let mut kept = kept();
        kept.held = kept.held.saturating_sub(1);
        if kept.held == 0 {
            kept.files.clear();
        }
    }
}

/// How many times this process decoded the file whose bytes have this digest: counted
/// for the last 64 files decoded, and none of a file not among them.
pub fn decodes_of(file: Digest) -> u64 {
    let kept = kept();
    let found = kept.decodes.iter().find(|(digest, _)| *digest == file);
    found.map_or(0, |(_, count)| *count)
}

fn counted(kept: &mut Kept, file: Digest) {
    if let Some(at) = kept.decodes.iter().position(|(digest, _)| *digest == file) {
        let (digest, count) = kept.decodes.remove(at).unwrap_or((file, 0));
        kept.decodes.push_back((digest, count + 1));
        return;
    }
    if kept.decodes.len() == COUNTED {
        kept.decodes.pop_front();
    }
    kept.decodes.push_back((file, 1));
}

/// Identifies by `decode`, or gives back the kept identification of the same file under
/// the same limits. The lock is never held while a file is decoded.
pub(super) fn identify(
    bytes: &[u8],
    limits: &DecodeLimits,
    decode: fn(&[u8], &DecodeLimits) -> Result<Identified>,
) -> Result<Arc<Identified>> {
    let keeping = kept().held > 0;
    let key = keeping.then(|| Digest::of_bytes(bytes));
    if let Some(key) = key {
        let mut kept = kept();
        let found =
            (kept.files.iter()).position(|(file, under, _)| *file == key && under == limits);
        if let Some(at) = found
            && let Some(entry) = kept.files.remove(at)
        {
            // The kept copy, shared: nothing of it is copied.
            let identified = Arc::clone(&entry.2);
            kept.files.push_back(entry);
            return Ok(identified);
        }
    }
    let identified = Arc::new(decode(bytes, limits)?);
    let mut kept = kept();
    counted(&mut kept, identified.identity.file.0);
    if let Some(key) = key
        && kept.held > 0
    {
        if kept.files.len() == KEPT {
            kept.files.pop_front();
        }
        kept.files
            .push_back((key, *limits, Arc::clone(&identified)));
    }
    Ok(identified)
}
