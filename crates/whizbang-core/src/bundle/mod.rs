//! Invocation bundles (ADR 0048; plan m7, task B1): a run, or a chain of runs, carried out
//! of one store as one file that another store takes in.
//!
//! A bundle is `bundle/1`, an uncompressed ustar ([`tar`]) that equal contents make byte
//! for byte: `bundle.json` first, the manifest ([`Manifest`]), then every other member in
//! the order of its name's bytes, so the archive's SHA-256 names it.
//!
//! | Member | Holds |
//! |---|---|
//! | `bundle.json` | The manifest, at most [`MANIFEST_MAX_BYTES`] |
//! | `runs/<run id>/record` | Each step's run record and each of its descendants', as the store holds it |
//! | `runs/<run id>/journal` | Its journal, as stored |
//! | `runs/<run id>/diagnostics/<name>` | Each diagnostic it kept |
//! | `files/sha256-<hex>` | Each canonical file of an input or an output, once for each file digest |
//!
//! [`make`] writes one from a store, through one snapshot; [`check`] reads one and holds
//! it to every rule of ADR 0048's "Reading one trusts nothing" before anything acts on it;
//! [`import`] brings a checked bundle into a store in one publication, every member
//! named under the bundle's entry, `bundles/<hex>/` ([`crate::store::BUNDLES_PREFIX`]);
//! and [`delete`] lets go of that entry whole. A bundle carries data, never code: a step
//! whose action this build does not ship is carried as evidence, not replayable.

pub mod tar;

mod import;
mod read;
mod write;

use serde::{Deserialize, Serialize};

use crate::artifact::ArtifactType;
use crate::identity::{Digest, PixelDigest};

pub use import::{ImportError, Imported, delete, entry, held, import, manifest_of};
pub use read::{Checked, check};
pub use write::{Made, make};

/// The bundle's kind and version, as its manifest's `bundle` holds it.
pub const FORMAT: u32 = 1;

/// The name of the manifest, the first member.
pub const MANIFEST: &str = "bundle.json";

/// The most bytes a bundle has.
pub const MAX_BYTES: u64 = 256 << 20;

/// The most members a bundle has, the manifest among them.
pub const MEMBERS_LIMIT: usize = 4096;

/// The most bytes of its manifest.
pub const MANIFEST_MAX_BYTES: u64 = 1 << 20;

/// The most steps a bundle has.
pub const STEPS_LIMIT: usize = crate::contract::record::BUNDLE_STEPS_LIMIT as usize;

/// What the name of a file member starts with: the hex of the file's SHA-256 follows.
pub const FILES_PREFIX: &str = "files/sha256-";

/// What the names of a run's members start with: the run's id follows, a `/`, and
/// `record`, `journal` or `diagnostics/<name>`.
pub const RUNS_PREFIX: &str = "runs/";

/// The action whose completed run keeps the painted answer it was given
/// (`whizbang_rascal::fill::NAME`), and the diagnostic it keeps it under
/// (`whizbang_rascal::fill::ANSWER`): a run of it without that diagnostic was made
/// before B1, and its answer is withheld. A test of the binary holds the two names to
/// rascal's.
pub const FILL_ACTION: &str = "fill";
pub const FILL_ANSWER: &str = "fill-answer.png";

/// The setting a run of `check` records whether it was answered from the decision cache
/// under (`whizbang_squint::cache::SETTING`), the word for a hit, and the setting that
/// names the run it was answered by (`whizbang_squint::cache::ANSWERED_BY`). A test of
/// the binary holds them to squint's.
pub const CACHE_SETTING: &str = "decision_cache";
pub const CACHE_HIT: &str = "hit";
pub const ANSWERED_BY: &str = "answered_by_run";

/// The manifest: what a bundle holds, and what it does not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// [`FORMAT`].
    pub bundle: u32,
    /// The version of whizbang that wrote it.
    pub whizbang: String,
    /// The root runs, in the order they ran, each with its family.
    pub steps: Vec<Step>,
    /// Each file member: its digest, the artifact it holds, and each run and port it is
    /// the file of.
    pub artifacts: Vec<File>,
    /// What the bundle does not carry, and why.
    pub withheld: Vec<Withheld>,
}

/// One step: a root run, a run nothing called.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    /// Its run's id.
    pub run: String,
    /// Its action and its action's version, as its record names them: null where the
    /// record lost its admission.
    #[serde(deserialize_with = "present")]
    pub action: Option<String>,
    #[serde(deserialize_with = "present")]
    pub version: Option<String>,
    /// The digest of the config its settings came from, as its record holds it.
    #[serde(deserialize_with = "present")]
    pub config: Option<Digest>,
    /// Whether a build of this action set can replay it, and why not when it cannot.
    pub replayable: bool,
    #[serde(deserialize_with = "present")]
    pub why: Option<String>,
    /// The runs of its family: the root first, then each run under it, at any depth.
    pub runs: Vec<String>,
    /// Each input of its record, in order: the artifact's identity, or the earlier
    /// step's output it took.
    pub inputs: Vec<StepInput>,
}

/// An input of a step: its port and its place among the inputs of that port, from 0,
/// and either the identity it took or the earlier step's output (exactly one of the two).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepInput {
    pub port: String,
    pub place: u32,
    #[serde(deserialize_with = "present")]
    pub artifact: Option<Digest>,
    #[serde(deserialize_with = "present")]
    pub from: Option<Link>,
}

/// The output of an earlier step an input took: the step, from 0, the port and the place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Link {
    pub step: u32,
    pub port: String,
    pub place: u32,
}

/// One file member: the SHA-256 of its bytes, the artifact it holds and its type, and
/// each run and port it is the file of.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct File {
    pub file: Digest,
    pub artifact: Digest,
    #[serde(rename = "type", with = "type_name")]
    pub artifact_type: ArtifactType,
    pub of: Vec<Use>,
}

/// Whether a file was a run's input or its output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Input,
    Output,
}

impl Role {
    /// The role in a word.
    pub fn name(self) -> &'static str {
        match self {
            Role::Input => "input",
            Role::Output => "output",
        }
    }
}

/// A run's input or output at a port and a place.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Use {
    pub run: String,
    #[serde(rename = "as")]
    pub role: Role,
    pub port: String,
    pub place: u32,
}

/// What a bundle does not carry, each with its reason in a sentence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "what", rename_all = "snake_case", deny_unknown_fields)]
pub enum Withheld {
    /// An opaque part of a judge's exchange, which the store keeps as its adapter, the
    /// digest of its body and its length (ADR 0009).
    OpaquePart {
        run: String,
        diagnostic: String,
        adapter: String,
        digest: Digest,
        bytes: u64,
        why: String,
    },
    /// A picture a stored exchange names that no image the bundle carries has.
    Picture {
        run: String,
        diagnostic: String,
        pixels: PixelDigest,
        why: String,
    },
    /// A diagnostic its record lists that the store no longer holds.
    Diagnostic {
        run: String,
        name: String,
        why: String,
    },
    /// A fill whose painted answer was not kept: a run made before B1.
    FillAnswer { run: String, why: String },
    /// A check answered from the decision cache, whose turns are another run's.
    CachedCheck {
        run: String,
        #[serde(deserialize_with = "present")]
        answered_by: Option<String>,
        why: String,
    },
    /// An input or an output whose file the store no longer holds.
    Artifact {
        run: String,
        #[serde(rename = "as")]
        role: Role,
        port: String,
        place: u32,
        artifact: Digest,
        why: String,
    },
}

impl Withheld {
    /// The run it is of.
    pub fn run(&self) -> &str {
        match self {
            Withheld::OpaquePart { run, .. }
            | Withheld::Picture { run, .. }
            | Withheld::Diagnostic { run, .. }
            | Withheld::FillAnswer { run, .. }
            | Withheld::CachedCheck { run, .. }
            | Withheld::Artifact { run, .. } => run,
        }
    }

    /// Its reason.
    pub fn why(&self) -> &str {
        match self {
            Withheld::OpaquePart { why, .. }
            | Withheld::Picture { why, .. }
            | Withheld::Diagnostic { why, .. }
            | Withheld::FillAnswer { why, .. }
            | Withheld::CachedCheck { why, .. }
            | Withheld::Artifact { why, .. } => why,
        }
    }
}

impl Manifest {
    /// The manifest as its member holds it: JSON on one line, then a newline.
    pub fn to_bytes(&self) -> Vec<u8> {
        // It holds strings, whole numbers and booleans: writing it cannot fail.
        let mut bytes = serde_json::to_vec(self).unwrap_or_default();
        bytes.push(b'\n');
        bytes
    }
}

/// Why a bundle was refused: its first fault. What it says ends by saying that nothing
/// was imported and nothing ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    pub fault: String,
}

impl Refused {
    pub(crate) fn new(fault: impl Into<String>) -> Refused {
        Refused {
            fault: fault.into(),
        }
    }
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the bundle was refused: {}; nothing was imported and nothing ran",
            self.fault
        )
    }
}

impl std::error::Error for Refused {}

/// A field that may be null and may not be left out.
fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(deserializer)
}

/// An artifact type as a manifest spells it: its name.
mod type_name {
    use serde::{Deserialize, Deserializer, Serializer};

    use crate::artifact::ArtifactType;

    pub fn serialize<S: Serializer>(artifact: &ArtifactType, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(artifact.name())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<ArtifactType, D::Error> {
        let name = String::deserialize(d)?;
        ArtifactType::ALL
            .into_iter()
            .find(|artifact| artifact.name() == name)
            .ok_or_else(|| serde::de::Error::custom("not the name of an artifact type"))
    }
}

/// The digest of a bundle's file, read as a stream and nothing else: its SHA-256, which
/// names it. A replay of a file looks its request id up by it before it imports anything
/// (ADR 0049).
pub fn file_digest(mut read: impl std::io::Read) -> std::io::Result<Digest> {
    use sha2::Digest as _;
    let mut hashed = sha2::Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let count = read.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hashed.update(&buffer[..count]);
    }
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&hashed.finalize());
    Ok(Digest::from_bytes(bytes))
}

/// A run's member names: `runs/<run>/` and what follows.
pub(crate) fn run_member(run: &str, name: &str) -> String {
    format!("{RUNS_PREFIX}{run}/{name}")
}

/// The name of a file member of this digest.
pub(crate) fn file_member(digest: &Digest) -> String {
    format!("{FILES_PREFIX}{}", digest.hex())
}
