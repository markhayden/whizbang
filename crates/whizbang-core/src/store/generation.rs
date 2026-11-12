//! A generation: everything the store holds at one moment, by name. Immutable once
//! published; the pointer names the current one.

use std::collections::BTreeMap;

use serde::de::{DeserializeOwned, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

use super::digest::Digest;

/// The version of the generation file's shape.
pub const GENERATION_FORMAT: u32 = 2;

/// What a reference to stored content is. The kind belongs to the reference, not to the
/// content: two references to the same bytes can differ, and each is treated as it says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A committed output: what an action produced, and what another may take as input.
    Output,
    /// Diagnostic evidence: kept and readable, and never usable as an output.
    Diagnostic,
}

/// One name's content: a reference to a stored file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// The stored file.
    pub digest: Digest,
    /// Its length. Reading checks it, so a file cut short is told from the right one
    /// before its digest is worked out, and a reader knows the size without reading.
    pub bytes: u64,
    /// What this reference is.
    pub kind: Kind,
}

/// What one run references, by names of its own. Deleting the run removes these
/// references and no others.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Run {
    #[serde(deserialize_with = "unique")]
    pub(crate) entries: BTreeMap<String, Entry>,
}

impl Run {
    /// Every name of the run, in order, with its content.
    pub fn entries(&self) -> &BTreeMap<String, Entry> {
        &self.entries
    }

    /// The content of one name.
    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.entries.get(name)
    }
}

/// One generation. Number 0 is the empty generation of a store that never published; it has
/// no file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Generation {
    pub(crate) format: u32,
    pub(crate) number: u64,
    #[serde(deserialize_with = "unique")]
    pub(crate) entries: BTreeMap<String, Entry>,
    #[serde(deserialize_with = "unique")]
    pub(crate) runs: BTreeMap<String, Run>,
}

impl Generation {
    pub(crate) fn empty() -> Generation {
        Generation {
            format: GENERATION_FORMAT,
            number: 0,
            entries: BTreeMap::new(),
            runs: BTreeMap::new(),
        }
    }

    /// The generation's number: 0 for the empty one, then 1, 2 and so on.
    pub fn number(&self) -> u64 {
        self.number
    }

    /// Every name that belongs to no run, in order, with its content. No command removes
    /// one of these: deleting a run removes that run's names only.
    pub fn entries(&self) -> &BTreeMap<String, Entry> {
        &self.entries
    }

    /// The content of one name that belongs to no run.
    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.entries.get(name)
    }

    /// Every run, in order of its id.
    pub fn runs(&self) -> &BTreeMap<String, Run> {
        &self.runs
    }

    /// One run.
    pub fn run(&self, id: &str) -> Option<&Run> {
        self.runs.get(id)
    }

    /// Every reference the generation holds, outside runs and in them.
    pub fn references(&self) -> impl Iterator<Item = &Entry> {
        self.entries
            .values()
            .chain(self.runs.values().flat_map(|run| run.entries.values()))
    }
}

/// A map of a generation file. A key that appears twice is refused: a map would keep the
/// last and hide the first.
fn unique<'de, D: Deserializer<'de>, V: DeserializeOwned>(
    deserializer: D,
) -> Result<BTreeMap<String, V>, D::Error> {
    struct Unique<V>(std::marker::PhantomData<V>);

    impl<'de, V: DeserializeOwned> Visitor<'de> for Unique<V> {
        type Value = BTreeMap<String, V>;

        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a map from names to what they hold")
        }

        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut found = BTreeMap::new();
            while let Some((name, value)) = map.next_entry::<String, V>()? {
                if found.contains_key(&name) {
                    return Err(serde::de::Error::custom(format!(
                        "the name {name:?} appears twice"
                    )));
                }
                found.insert(name, value);
            }
            Ok(found)
        }
    }

    deserializer.deserialize_map(Unique(std::marker::PhantomData))
}
