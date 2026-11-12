//! The name of stored content: the SHA-256 of its bytes. It is the one digest type of the
//! core ([`crate::identity::Digest`]). Its text everywhere is `sha256:` and 64 hex digits;
//! the store writes the 64 digits alone in two places: the name of a stored file, and a
//! generation file's entries.

pub use crate::identity::Digest;

/// A digest in a generation file: the 64 hex digits alone.
pub(super) mod bare_hex {
    use serde::{Deserialize, Deserializer, Serializer};

    use super::Digest;

    pub fn serialize<S: Serializer>(digest: &Digest, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&digest.hex())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Digest, D::Error> {
        let text = String::deserialize(deserializer)?;
        Digest::from_hex(&text)
            .ok_or_else(|| serde::de::Error::custom("a digest is 64 lower-case hex digits"))
    }
}
