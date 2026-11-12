//! The name of stored content: the SHA-256 of its bytes.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest as _, Sha256};

/// The SHA-256 of a stored file's bytes. Written as 64 lower-case hex digits, which is also
/// the file's name in the store.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Digest([u8; 32]);

impl Digest {
    /// The digest of these bytes.
    pub fn of(bytes: &[u8]) -> Digest {
        Digest(Sha256::digest(bytes).into())
    }

    /// Reads the written form: exactly 64 hex digits, in lower case.
    pub fn from_hex(text: &str) -> Option<Digest> {
        let text = text.as_bytes();
        if text.len() != 64 {
            return None;
        }
        let nibble = |c: u8| match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            _ => None,
        };
        let mut bytes = [0u8; 32];
        for (byte, [high, low]) in bytes.iter_mut().zip(text.as_chunks::<2>().0) {
            *byte = nibble(*high)? << 4 | nibble(*low)?;
        }
        Some(Digest(bytes))
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Digest({self})")
    }
}

impl Serialize for Digest {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Digest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Digest, D::Error> {
        let text = String::deserialize(deserializer)?;
        Digest::from_hex(&text)
            .ok_or_else(|| serde::de::Error::custom("a digest is 64 lower-case hex digits"))
    }
}
