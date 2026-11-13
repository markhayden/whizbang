//! The capability interfaces (`SPEC.md` section 4, ADR 0006): the request and the
//! response of segment, fill and judge, what a backend declares, and how a call's choice
//! of backend is resolved.
//!
//! Nothing here implements a capability. A backend does, in another crate, and a fake
//! does for the tests. An action reaches one through a handle in its
//! [`Context`](crate::contract::Context), and the handle holds the request to what the
//! backend declared and the answer to the request.
//!
//! What a backend sends back is not trusted. Every type a response is made of is built
//! through a constructor that checks it, has private fields, and is never deserialised,
//! so a value of the type is a value that passed: a support plane in range, a fill of
//! three bytes a pixel, a turn with something in it. What does not pass is [`Unfit`].

use std::sync::Arc;

use crate::identity::{CanonicalPixels, PixelDigest};

mod declare;
mod fault;
mod fill;
mod segment;

pub use declare::{Declaration, Resolved, Roster, RosterError, Unresolved, feature};
pub use fault::{FAULT_LIMIT, Fault, FaultKind};
pub use fill::{FillHandle, FillRequest, Filler, PROMPT_LIMIT, Painted, Support};
pub use segment::{
    Asked, INSTANCE_LIMIT, Instance, PHRASE_LIMIT, QUERY_LIMIT, Query, SegmentHandle,
    SegmentRequest, SegmentResponse, Segmenter,
};

/// The most pixels an image, a plane or a fill may have: ADR 0001's default limit on a
/// decoded image, held here for everything a capability is sent or sends back.
pub const PIXEL_LIMIT: u64 = 50_000_000;

/// Why a value is not one a capability's request or response may hold. Each is a plain
/// sentence made of fixed words and numbers: it quotes nothing a backend sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unfit {
    /// Nothing where something is required: no pixels, no text, no content.
    Empty { what: &'static str },
    /// Over a bound.
    TooLarge {
        what: &'static str,
        size: u64,
        limit: u64,
    },
    /// Not as many bytes as its dimensions and its layout give: another channel count,
    /// or a plane cut short.
    WrongLength {
        what: &'static str,
        expected: u64,
        found: u64,
    },
    /// Not the dimensions of the image it belongs to.
    WrongSize {
        what: &'static str,
        expected: (u32, u32),
        found: (u32, u32),
    },
    /// A value outside its range: the first one, where it is when it is one of many,
    /// and what is allowed.
    OutOfRange {
        what: &'static str,
        at: Option<u64>,
        found: String,
        allowed: &'static str,
    },
    /// Something that may not be where it is.
    NotAllowed {
        what: &'static str,
        why: &'static str,
    },
}

impl std::fmt::Display for Unfit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unfit::Empty { what } => write!(f, "{what} is empty"),
            Unfit::TooLarge { what, size, limit } => {
                write!(f, "{what} is {size}, over the limit of {limit}")
            }
            Unfit::WrongLength {
                what,
                expected,
                found,
            } => write!(f, "{what} has {found} bytes, not {expected}"),
            Unfit::WrongSize {
                what,
                expected,
                found,
            } => write!(
                f,
                "{what} is {} by {}, not {} by {}",
                found.0, found.1, expected.0, expected.1
            ),
            Unfit::OutOfRange {
                what,
                at: Some(at),
                found,
                allowed,
            } => write!(f, "{what} holds {found} at {at}: a value is {allowed}"),
            Unfit::OutOfRange {
                what,
                at: None,
                found,
                allowed,
            } => write!(f, "{what} is {found}: a value is {allowed}"),
            Unfit::NotAllowed { what, why } => write!(f, "{what} {why}"),
        }
    }
}

impl std::error::Error for Unfit {}

/// The canonical pixels of an image (ADR 0001), as a capability is sent them: four bytes
/// a pixel, red, green, blue and straight alpha, sRGB, row by row from the top left.
///
/// A clone shares the pixels. What `{:?}` prints is the digest and the size, never the
/// pixels. Two pictures are equal when their digests are.
#[derive(Clone)]
pub struct Picture {
    digest: PixelDigest,
    pixels: Arc<CanonicalPixels>,
}

impl Picture {
    /// The picture of these pixels, within [`PIXEL_LIMIT`]. Its digest is worked out
    /// here, once.
    pub fn new(pixels: CanonicalPixels) -> Result<Picture, Unfit> {
        let size = u64::from(pixels.width()) * u64::from(pixels.height());
        if size > PIXEL_LIMIT {
            return Err(Unfit::TooLarge {
                what: "the image, in pixels,",
                size,
                limit: PIXEL_LIMIT,
            });
        }
        Ok(Picture {
            digest: pixels.digest(),
            pixels: Arc::new(pixels),
        })
    }

    pub fn width(&self) -> u32 {
        self.pixels.width()
    }

    pub fn height(&self) -> u32 {
        self.pixels.height()
    }

    /// The canonical pixel digest (ADR 0001).
    pub fn digest(&self) -> PixelDigest {
        self.digest
    }

    pub fn pixels(&self) -> &CanonicalPixels {
        &self.pixels
    }

    /// Red, green, blue and alpha for each pixel, row by row from the top left.
    pub fn rgba(&self) -> &[u8] {
        self.pixels.rgba()
    }
}

impl PartialEq for Picture {
    fn eq(&self, other: &Picture) -> bool {
        self.digest == other.digest
    }
}

impl Eq for Picture {}

impl std::fmt::Debug for Picture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Picture({} by {}, {})",
            self.width(),
            self.height(),
            self.digest
        )
    }
}

/// The count of pixels of an image this wide and this high, when it is one a
/// capability may hold: neither side zero, and within [`PIXEL_LIMIT`].
pub(crate) fn pixel_count(what: &'static str, width: u32, height: u32) -> Result<u64, Unfit> {
    let size = u64::from(width) * u64::from(height);
    if size == 0 {
        return Err(Unfit::Empty { what });
    }
    if size > PIXEL_LIMIT {
        return Err(Unfit::TooLarge {
            what,
            size,
            limit: PIXEL_LIMIT,
        });
    }
    Ok(size)
}
