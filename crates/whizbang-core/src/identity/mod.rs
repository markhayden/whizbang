//! Identities (`SPEC.md` section 3, ADR 0001): what an image is, what space its coordinates
//! live in, and how one image sits on another.
//!
//! - An [`ImageIdentity`] is two digests: of the file's bytes, and of its canonical pixels.
//! - A [`Space`] is dimensions, projection and topology, with no content.
//! - A [`Binding`] places a source image onto a destination through a [`Mapping`] with a
//!   stated [`Direction`]. A mask or a piece of evidence belongs to a binding.
//!
//! Identities depend on each other in one order: content first (the digests), bindings next,
//! run provenance last. Nothing local enters any of them: no id, timestamp or path.

mod binding;
mod colour;
mod digest;
mod mapping;
mod pixels;
mod space;

pub use binding::{Binding, BindingId, Destination, Direction};
pub use digest::{Digest, FileDigest, PixelDigest};
pub use mapping::Mapping;
pub use pixels::{
    CanonicalPixels, DecodeLimits, Identified, ImageIdentity, Profile, Stored, identify,
};
pub use space::{Point, Projection, Space, Topology};

/// Why an identity could not be made, or a point could not be mapped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityError {
    /// A digest's text is not `sha256:` and 64 lower-case hex digits.
    Digest(String),
    /// The bytes are not an image that can be decoded.
    Image(String),
    /// The image is over a decode limit; nothing was decoded.
    Limit(String),
    /// The image decodes, but ADR 0001 defines no canonical pixels for it.
    Unsupported(String),
    /// The dimensions, projection and topology do not make a space.
    Space(String),
    /// The mapping does not fit the two spaces, or cannot be inverted.
    Binding(String),
    /// The point has no place in the space it was given in, or in the one it maps to.
    Outside(String),
}

impl std::fmt::Display for IdentityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (what, why) = match self {
            IdentityError::Digest(why) => ("digest", why),
            IdentityError::Image(why) => ("image", why),
            IdentityError::Limit(why) => ("decode limit", why),
            IdentityError::Unsupported(why) => ("unsupported image", why),
            IdentityError::Space(why) => ("space", why),
            IdentityError::Binding(why) => ("binding", why),
            IdentityError::Outside(why) => ("point", why),
        };
        write!(f, "{what}: {why}")
    }
}

impl std::error::Error for IdentityError {}

/// The result of this module's fallible functions.
pub type Result<T> = std::result::Result<T, IdentityError>;
