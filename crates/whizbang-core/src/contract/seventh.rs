//! What `run/7` adds to a run's record (plan m7, task B1, item 6; ADR 0048, "The record
//! a bundle carries"): a model's card, in its entry of `models`, and `replayed`, the run
//! this run replays, a group after the attempts of `run/6`. Each is written only where
//! the run has it; a record that has neither is written as the version before, which
//! holds all of it.
//!
//! The types are part of the record ([`super::record`] names them again), and are read
//! from JSON only as part of a stored record.

use serde::{Deserialize, Serialize};

use crate::identity::Digest;

use super::RunId;

/// The most steps a bundle holds: a chain of 16 runs at most (plan m7, "Guided chain
/// replay"; `whizbang runs bundle`).
pub const BUNDLE_STEPS_LIMIT: u32 = 16;

/// How a run replays another: answered from what the run recorded and calling nothing,
/// or made again for real as a new run (plan m7, "Replay").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayMode {
    Offline,
    Live,
}

/// The bundle a replayed run came from: its digest, the SHA-256 of its file, and the
/// step it is, from 0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FromBundle {
    pub digest: Digest,
    pub step: u32,
}

/// The run a run replays (ADR 0048): its id, the bundle and step it came from when it
/// came from one, and whether it is replayed offline or live. Every run of a replayed
/// family carries it, a child as its root does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Replayed {
    pub run: String,
    pub bundle: Option<FromBundle>,
    pub mode: ReplayMode,
}

impl Replayed {
    /// Whether a record holds it: the run it names is a run's id, and the step is one a
    /// bundle has.
    pub(super) fn holds(&self) -> bool {
        RunId::parse(&self.run).is_ok()
            && (self.bundle.as_ref()).is_none_or(|bundle| bundle.step < BUNDLE_STEPS_LIMIT)
    }
}

/// A field that may be null and may not be left out.
fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(deserializer)
}

#[derive(Deserialize)]
#[serde(remote = "Replayed", deny_unknown_fields)]
struct ReplayedWire {
    run: String,
    #[serde(deserialize_with = "present")]
    bundle: Option<FromBundle>,
    mode: ReplayMode,
}

/// `replayed`, as a stored record holds it.
pub(super) fn replayed_wire<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Replayed>, D::Error> {
    #[derive(Deserialize)]
    struct One(#[serde(with = "ReplayedWire")] Replayed);
    Ok(Option::<One>::deserialize(deserializer)?.map(|One(replayed)| replayed))
}
