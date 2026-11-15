//! The check behind a send (`SPEC.md` section 10, "Egress"; ADR 0019; plan 1B, tasks
//! E2 and E6): the [`HopCheck`] an executor that sends is given.
//!
//! The executor asks four things itself before it asks a check, of every address
//! (ADR 0015): the backend declared a recipient, the request brings no credential,
//! the address is at the recipient's own origin or one of its upstreams, and the
//! policy approves the recipient for every class of data the request holds. A check
//! is asked last, and can only refuse.
//!
//! **This check approves a first address, and no address an answer pointed to.**
//! Following an answer that points elsewhere is task E3's.
//!
//! **And it looks at what would leave** (task E6): an image leaves only as the
//! encoder writes its pixels (`pixels.rs`), and no part of a request holds an
//! absolute path or a folder the check was told to keep out.

use std::fmt;
use std::path::Path;

use super::pixels::{SIGNATURE, is_encoded};
use super::{Denied, Hop, HopCheck, NotAllowed};

/// The most folders a check keeps out.
pub const KEPT_OUT_LIMIT: usize = 8;

/// What the check answers once an answer has pointed its backend elsewhere.
const MOVED: &str = "an answer pointed this backend elsewhere in this run, and the address \
                     of no request after that is approved";

/// What it answers of a part that holds a path.
const PATH: &str = "a part of the request holds an absolute path, and no path leaves the \
                    machine";

/// What it answers of a request that holds a folder it was told to keep out.
const KEPT_OUT: &str = "the request holds the path of a folder that is kept out of what \
                        leaves the machine";

/// What it answers of a part that is an image file the encoder did not write.
const NOT_PIXELS: &str = "a part of the request is an image file that whizbang's encoder \
                          did not write, and an image leaves only as its pixels";

/// How an absolute path starts, where it names a folder every machine of its kind
/// has: a person's folders, the temporary folders, and an address that names a file.
const ROOTS: [&[u8]; 6] = [
    b"/Users/",
    b"/home/",
    b"/tmp/",
    b"/var/folders/",
    b"/private/",
    b"file://",
];

/// Whether `bytes` hold `text` anywhere. `text` is never empty here.
fn holds(bytes: &[u8], text: &[u8]) -> bool {
    bytes.windows(text.len()).any(|window| window == text)
}

/// Whether the bytes hold an absolute path: one of [`ROOTS`], or a drive, which is
/// one letter, a colon and a backslash, where the letter is not the end of a longer
/// word or number.
fn holds_a_path(bytes: &[u8]) -> bool {
    if ROOTS.iter().any(|root| holds(bytes, root)) {
        return true;
    }
    let drive = |at: usize| {
        let before = at.checked_sub(1).and_then(|before| bytes.get(before));
        let alone = !before.is_some_and(u8::is_ascii_alphanumeric);
        let letter = bytes.get(at).is_some_and(u8::is_ascii_alphabetic);
        alone && letter && bytes.get(at + 1..at + 3) == Some(b":\\")
    };
    (0..bytes.len()).any(drive)
}

/// Whether the bytes start as an image file does: a PNG, a JPEG, a GIF, a TIFF or a
/// WebP.
fn starts_as_an_image(bytes: &[u8]) -> bool {
    let starts: [&[u8]; 6] = [
        &SIGNATURE,
        b"\xff\xd8\xff",
        b"GIF87a",
        b"GIF89a",
        b"II*\x00",
        b"MM\x00*",
    ];
    let webp = bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP");
    webp || starts.iter().any(|start| bytes.starts_with(start))
}

/// The check behind a send. It approves an address while no answer has pointed its
/// backend elsewhere in the run, and refuses every address after one has.
///
/// It goes by [`Hop::total_moves`], which never goes back, and not by the moves in a
/// row: a request of the same backend that is answered meanwhile sets those back to
/// nothing, and the address an answer pointed to would then pass for a first one.
/// So once one answer of a backend has pointed elsewhere, every later request of
/// that backend in that run is refused, a first address among them.
///
/// Of what would leave, it refuses, in fixed words that show nothing of it:
///
/// - a part that starts as an image file does and is not, byte for byte, what the
///   encoder writes of a picture ([`Piece::picture`](super::Piece::picture)). A part
///   that is, is pixels, and is not read as words;
/// - a part, or an address, that holds the path of a folder the check was told to
///   keep out ([`SendCheck::keeping_out`]): the store's root;
/// - a part that holds an absolute path.
///
/// It holds nothing that changes and counts nothing: asked for a run that is over,
/// or twice for one request, it changes nothing. What `{:?}` prints is how many
/// folders it keeps out, never one of them.
#[derive(Clone, Default)]
pub struct SendCheck {
    /// Each folder's path, as it is written.
    kept_out: Vec<Vec<u8>>,
}

impl SendCheck {
    /// The check, told of no folder to keep out.
    pub fn new() -> SendCheck {
        SendCheck::default()
    }

    /// The same check, which also refuses a request that holds this folder's path,
    /// as it is written here: the root of the executor's store. Refused: a path of
    /// nothing, and one more folder than [`KEPT_OUT_LIMIT`].
    pub fn keeping_out(mut self, folder: &Path) -> Result<SendCheck, NotAllowed> {
        let written = folder.as_os_str().as_encoded_bytes();
        if written.is_empty() {
            return Err(NotAllowed::Empty);
        }
        if self.kept_out.len() >= KEPT_OUT_LIMIT {
            return Err(NotAllowed::TooMany);
        }
        self.kept_out.push(written.to_vec());
        Ok(self)
    }

    fn keeps_out(&self, bytes: &[u8]) -> bool {
        self.kept_out.iter().any(|folder| holds(bytes, folder))
    }

    /// One part of what would leave.
    fn part(&self, bytes: &[u8]) -> Result<(), Denied> {
        if is_encoded(bytes) {
            return Ok(());
        }
        if starts_as_an_image(bytes) {
            return Err(Denied::because(NOT_PIXELS));
        }
        if self.keeps_out(bytes) {
            return Err(Denied::because(KEPT_OUT));
        }
        if holds_a_path(bytes) {
            return Err(Denied::because(PATH));
        }
        Ok(())
    }
}

impl fmt::Debug for SendCheck {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SendCheck({} folders kept out)", self.kept_out.len())
    }
}

impl HopCheck for SendCheck {
    fn look(&self, hop: &Hop<'_>) -> Result<(), Denied> {
        if hop.total_moves() > 0 {
            return Err(Denied::because(MOVED));
        }
        if self.keeps_out(hop.to().rest().as_bytes()) {
            return Err(Denied::because(KEPT_OUT));
        }
        hop.pieces()
            .iter()
            .try_for_each(|piece| self.part(piece.bytes()))
    }
}
