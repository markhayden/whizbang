//! A generation: everything the store holds at one moment, by name. Immutable once
//! published; the pointer names the current one.

use std::collections::BTreeMap;

use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

use super::digest::Digest;

/// The version of the generation file's shape.
pub const GENERATION_FORMAT: u32 = 1;

/// One name's content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// The stored file.
    pub digest: Digest,
    /// Its length. Reading checks it, so a file cut short is told from the right one
    /// before its digest is worked out, and a reader knows the size without reading.
    pub bytes: u64,
}

/// One generation. Number 0 is the empty generation of a store that never published; it has
/// no file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Generation {
    pub(crate) format: u32,
    pub(crate) number: u64,
    #[serde(deserialize_with = "unique_entries")]
    pub(crate) entries: BTreeMap<String, Entry>,
}

impl Generation {
    pub(crate) fn empty() -> Generation {
        Generation {
            format: GENERATION_FORMAT,
            number: 0,
            entries: BTreeMap::new(),
        }
    }

    /// The generation's number: 0 for the empty one, then 1, 2 and so on.
    pub fn number(&self) -> u64 {
        self.number
    }

    /// Every name the generation holds, in order, with its content.
    pub fn entries(&self) -> &BTreeMap<String, Entry> {
        &self.entries
    }

    /// The content of one name.
    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.entries.get(name)
    }
}

/// The entries of a generation file. A name that appears twice is refused: a map would
/// keep the last and hide the first.
fn unique_entries<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, Entry>, D::Error> {
    struct Entries;

    impl<'de> Visitor<'de> for Entries {
        type Value = BTreeMap<String, Entry>;

        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a map from names to entries")
        }

        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut entries = BTreeMap::new();
            while let Some((name, entry)) = map.next_entry::<String, Entry>()? {
                if entries.contains_key(&name) {
                    return Err(serde::de::Error::custom(format!(
                        "the name {name:?} appears twice"
                    )));
                }
                entries.insert(name, entry);
            }
            Ok(entries)
        }
    }

    deserializer.deserialize_map(Entries)
}
