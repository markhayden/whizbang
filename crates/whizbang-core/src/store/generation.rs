//! A generation: everything the store holds at one moment, by name. Immutable once
//! published; the pointer names the current one.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::digest::Digest;

/// The version of the generation file's shape.
pub const GENERATION_FORMAT: u32 = 1;

/// One name's content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// The stored file.
    pub digest: Digest,
    /// Its length.
    pub bytes: u64,
}

/// One generation. Number 0 is the empty generation of a store that never published; it has
/// no file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Generation {
    pub(crate) format: u32,
    pub(crate) number: u64,
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
