//! SHA-256 digests, and the one encoding every identity is hashed from (ADR 0001).

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::{IdentityError, Result};

/// A SHA-256 digest. Its text is `sha256:` and 64 lower-case hex digits.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Digest([u8; 32]);

impl Digest {
    /// The digest of these bytes as they are, with nothing added.
    pub fn of_bytes(bytes: &[u8]) -> Digest {
        Digest(Sha256::digest(bytes).into())
    }

    /// The 32 bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// The 64 lower-case hex digits, without the `sha256:` prefix.
    pub fn hex(&self) -> String {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(64);
        for byte in self.0 {
            out.push(char::from(DIGITS[usize::from(byte >> 4)]));
            out.push(char::from(DIGITS[usize::from(byte & 15)]));
        }
        out
    }
}

impl std::fmt::Display for Digest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "sha256:{}", self.hex())
    }
}

impl std::fmt::Debug for Digest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self}")
    }
}

impl std::str::FromStr for Digest {
    type Err = IdentityError;

    fn from_str(text: &str) -> Result<Digest> {
        let refuse = || {
            IdentityError::Digest(format!(
                "{text:?} is not sha256: and 64 lower-case hex digits"
            ))
        };
        let hex = text.strip_prefix("sha256:").ok_or_else(refuse)?.as_bytes();
        if hex.len() != 64 {
            return Err(refuse());
        }
        let nibble = |digit: u8| match digit {
            b'0'..=b'9' => Some(digit - b'0'),
            b'a'..=b'f' => Some(digit - b'a' + 10),
            _ => None,
        };
        let mut bytes = [0u8; 32];
        for (byte, pair) in bytes.iter_mut().zip(hex.as_chunks::<2>().0) {
            let (high, low) = nibble(pair[0]).zip(nibble(pair[1])).ok_or_else(refuse)?;
            *byte = (high << 4) | low;
        }
        Ok(Digest(bytes))
    }
}

impl TryFrom<String> for Digest {
    type Error = IdentityError;

    fn try_from(text: String) -> Result<Digest> {
        text.parse()
    }
}

impl From<Digest> for String {
    fn from(digest: Digest) -> String {
        digest.to_string()
    }
}

/// The digest of a file's bytes, exactly as stored: what `shasum -a 256` prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FileDigest(pub Digest);

/// The digest of an image's canonical pixels (ADR 0001): equal for the same picture however
/// it is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PixelDigest(pub Digest);

impl std::fmt::Display for FileDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::fmt::Display for PixelDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Hashes one identity. The bytes hashed are `whizbang:`, the kind (which carries its
/// version, as in `pixels/1`), a zero byte, then each field in a fixed order. A field is one
/// tag byte and its value: `u` and eight bytes, big-endian; `f` and the eight bytes of an
/// IEEE 754 double, big-endian, with negative zero written as zero; `d` and the 32 bytes of
/// a digest; `b`, the length as eight bytes, big-endian, and the bytes.
pub(super) struct Encoder(Sha256);

impl Encoder {
    pub(super) fn new(kind: &str) -> Encoder {
        let mut hasher = Sha256::new();
        hasher.update(b"whizbang:");
        hasher.update(kind.as_bytes());
        hasher.update([0]);
        Encoder(hasher)
    }

    pub(super) fn unsigned(&mut self, value: u64) {
        self.0.update(b"u");
        self.0.update(value.to_be_bytes());
    }

    /// The caller has refused a value that is not finite.
    pub(super) fn float(&mut self, value: f64) {
        let value = if value == 0.0 { 0.0 } else { value };
        self.0.update(b"f");
        self.0.update(value.to_bits().to_be_bytes());
    }

    pub(super) fn digest(&mut self, value: &Digest) {
        self.0.update(b"d");
        self.0.update(value.as_bytes());
    }

    pub(super) fn finish(self) -> Digest {
        Digest(self.0.finalize().into())
    }
}
