//! The status record the holder of a model's lock publishes as it goes (ADR 0017,
//! "The status record"): `models/<model>/STATUS`, which says whether the model is
//! being installed or removed, which installation an install is of and how far it
//! is, and how the last install or removal ended.
//!
//! It is written for whoever is not the holder: a second request that attaches to
//! a running install ([`super::install_or_attach`]), and a person. No install, hold
//! or removal is decided from it; a request that attached is told from it how the
//! install ended. Whether an install or a removal is running is said by the model's
//! lock; which installation is in use is said by the pointer. So it is given no sync
//! of its own, and a crash can leave one that says "installing" or "removing" with
//! nobody alive: a status that says so while the model's lock is free is the status
//! of one that died.

use std::io;

use serde::Serialize;
use whizbang_core::contract::Secrets;

use super::catalogue::FILES_LIMIT;
use super::directory::Read;
use super::json::{self, Object};
use super::{InstallationId, ModelName, ModelsDir, Refused, Rule, What};

/// The format of a status record.
pub const STATUS_FORMAT: &str = "status/2";

/// The most bytes a status record may be. The longest one whizbang writes is under
/// 160.
pub const STATUS_MAX_BYTES: u64 = 256;

/// What the holder of the model's lock is doing, or how the last one ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// An install is staging the model's files, or publishing them.
    Installing,
    /// The last install ended with the pointer naming its installation.
    Installed,
    /// The last install was refused: a file, the disk, a hold, a removal.
    Failed,
    /// The last install was cancelled.
    Cancelled,
    /// A removal has the model's lock: it waits for the holds, or has cleared the
    /// pointer.
    Removing,
    /// The last removal cleared the pointer.
    Removed,
}

impl State {
    /// Whether this is a state of a removal, which names no installation and counts
    /// no files.
    pub fn is_of_a_removal(self) -> bool {
        matches!(self, State::Removing | State::Removed)
    }
}

/// Each state with the word it is written as.
const STATES: [(State, &str); 6] = [
    (State::Installing, "installing"),
    (State::Installed, "installed"),
    (State::Failed, "failed"),
    (State::Cancelled, "cancelled"),
    (State::Removing, "removing"),
    (State::Removed, "removed"),
];

/// What a status says of an install: the installation it is of, and how many of the
/// entry's files were staged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Install {
    installation: InstallationId,
    files_done: u64,
    files: u64,
}

/// A status record: a state; and, of an install, the identity of the installation it
/// is of and how many of the entry's files were staged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    state: State,
    install: Option<Install>,
}

#[derive(Serialize)]
struct Written<'a> {
    format: &'a str,
    state: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    installation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    files_done: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    files: Option<u64>,
}

impl Status {
    /// A status of an install of this installation, in one of the four states of an
    /// install, with this many of this many files staged: 1 to [`FILES_LIMIT`]
    /// files, and no more of them staged than there are.
    pub fn of_install(
        state: State,
        installation: InstallationId,
        files_done: u64,
        files: u64,
    ) -> Result<Status, Refused> {
        if state.is_of_a_removal() {
            return Err(Refused::at("state", Rule::State));
        }
        if files == 0 {
            return Err(Refused::at("files", Rule::NoFiles));
        }
        if files > FILES_LIMIT as u64 {
            return Err(Refused::at("files", Rule::Count { most: FILES_LIMIT }));
        }
        if files_done > files {
            return Err(Refused::at("files_done", Rule::Progress));
        }
        let install = Install {
            installation,
            files_done,
            files,
        };
        Ok(Status {
            state,
            install: Some(install),
        })
    }

    /// A status of a removal, in one of its two states. It names no installation and
    /// counts no files.
    pub fn of_removal(state: State) -> Result<Status, Refused> {
        if !state.is_of_a_removal() {
            return Err(Refused::at("state", Rule::State));
        }
        Ok(Status {
            state,
            install: None,
        })
    }

    /// Reads these bytes as a status record, as an installation's record is read:
    /// the length first, then the bytes whole for a secret, then strictly, and last
    /// held to the one form whizbang writes. A removal's record has the format and
    /// the state and no other field; an install's has all five.
    pub fn parse(bytes: &[u8], secrets: &Secrets) -> Result<Status, Refused> {
        let what = What::Status;
        let json = json::parse(what, bytes, STATUS_MAX_BYTES, secrets)?;
        let mut top = Object::of(json, "")?;
        if top.text("format")? != STATUS_FORMAT {
            return Err(Refused::at("format", Rule::Format));
        }
        let word = top.text("state")?;
        let state = STATES
            .iter()
            .find(|(_, written)| *written == word)
            .map(|(state, _)| *state)
            .ok_or_else(|| Refused::at("state", Rule::State))?;
        let status = if state.is_of_a_removal() {
            top.done()?;
            Status::of_removal(state)?
        } else {
            let installation = match InstallationId::from_text(&top.text("installation")?, secrets)
            {
                Ok(installation) => installation,
                Err(Refused::HoldsSecret) => return Err(Refused::HoldsSecret),
                Err(_) => return Err(Refused::at("installation", Rule::Hash)),
            };
            let files_done = top.whole("files_done")?;
            let files = top.whole("files")?;
            top.done()?;
            Status::of_install(state, installation, files_done, files)?
        };
        if status.to_bytes() != bytes {
            return Err(Refused::OtherForm { what });
        }
        Ok(status)
    }

    /// The record in its one form: compact JSON, the fields in the order of the
    /// format, nothing after the brace.
    pub fn to_bytes(&self) -> Vec<u8> {
        let state = STATES
            .iter()
            .find(|(state, _)| *state == self.state)
            .map_or("", |(_, written)| written);
        let written = Written {
            format: STATUS_FORMAT,
            state,
            installation: self.install.map(|install| install.installation.to_string()),
            files_done: self.install.map(|install| install.files_done),
            files: self.install.map(|install| install.files),
        };
        // Writing three texts and two whole numbers as JSON cannot fail.
        serde_json::to_vec(&written).unwrap_or_default()
    }

    pub fn state(&self) -> State {
        self.state
    }

    /// The installation an install is of, or was; none of a removal.
    pub fn installation(&self) -> Option<InstallationId> {
        self.install.map(|install| install.installation)
    }

    /// How many of the entry's files were staged and held to their size and hash.
    /// Nothing, of a removal.
    pub fn files_done(&self) -> u64 {
        self.install.map_or(0, |install| install.files_done)
    }

    /// How many files the entry has. Nothing, of a removal.
    pub fn files(&self) -> u64 {
        self.install.map_or(0, |install| install.files)
    }
}

/// The model's status record: none when nobody ever wrote one. A record that is not
/// a regular file under its own name, is over its bound or is not in the one form is
/// refused, never followed and never guessed at.
pub fn status(
    dir: &ModelsDir,
    model: &ModelName,
    secrets: &Secrets,
) -> Result<Option<Status>, Refused> {
    let what = What::Status;
    match dir.read(&dir.model(model).join("STATUS"), STATUS_MAX_BYTES) {
        Ok(Read::Bytes(bytes)) => Status::parse(&bytes, secrets).map(Some),
        Ok(Read::Missing) => Ok(None),
        Ok(Read::NotAFile) => Err(Refused::Unreadable {
            what,
            kind: io::ErrorKind::InvalidData,
        }),
        Err(error) => Err(Refused::Unreadable {
            what,
            kind: error.kind(),
        }),
    }
}

/// Replaces the model's status record, for one who holds the model's lock. The
/// record is written whole beside the old one and renamed over it; the rename is
/// given no sync of its own.
pub(super) fn write(dir: &ModelsDir, model: &ModelName, status: &Status) -> io::Result<()> {
    let folder = dir.model(model);
    let new = folder.join("STATUS.new");
    dir.remove(&new)?;
    dir.fs().write_new(&new, &status.to_bytes())?;
    dir.fs().rename(&new, &folder.join("STATUS"))
}

/// Takes the model's status record away, for one who holds the model's lock: there
/// is none again, as before anybody wrote one.
pub(super) fn clear(dir: &ModelsDir, model: &ModelName) -> io::Result<()> {
    dir.remove(&dir.model(model).join("STATUS"))
}
