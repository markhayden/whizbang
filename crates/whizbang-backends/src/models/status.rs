//! The status record an installer publishes as it goes (ADR 0017, "The status
//! record"): `models/<model>/STATUS`, which says whether the model is being installed,
//! how far the install is, and how the last one ended.
//!
//! It is written for whoever is not the installer: a second request that attaches to
//! a running install (task M3), and a person. Nothing is decided from it. Whether an
//! install is running is said by the model's lock; which installation is in use is
//! said by the pointer. So it is given no sync of its own, and a crash can leave one that
//! says "installing" with no installer alive: a status that says so while the model's
//! lock is free is the status of an install that died.

use std::io;

use serde::Serialize;
use whizbang_core::contract::Secrets;

use super::catalogue::FILES_LIMIT;
use super::directory::Read;
use super::json::{self, Object};
use super::{ModelName, ModelsDir, Refused, Rule, What};

/// The format of a status record.
pub const STATUS_FORMAT: &str = "status/1";

/// The most bytes a status record may be. The longest one whizbang writes is under
/// 80.
pub const STATUS_MAX_BYTES: u64 = 256;

/// What an install is doing, or how it ended.
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
}

/// Each state with the word it is written as.
const STATES: [(State, &str); 4] = [
    (State::Installing, "installing"),
    (State::Installed, "installed"),
    (State::Failed, "failed"),
    (State::Cancelled, "cancelled"),
];

/// A status record: a state, and how many of the entry's files were staged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    state: State,
    files_done: u64,
    files: u64,
}

#[derive(Serialize)]
struct Written<'a> {
    format: &'a str,
    state: &'a str,
    files_done: u64,
    files: u64,
}

impl Status {
    /// A status of this state, with this many of this many files staged: 1 to
    /// [`FILES_LIMIT`] files, and no more of them staged than there are.
    pub fn new(state: State, files_done: u64, files: u64) -> Result<Status, Refused> {
        if files == 0 {
            return Err(Refused::at("files", Rule::NoFiles));
        }
        if files > FILES_LIMIT as u64 {
            return Err(Refused::at("files", Rule::Count { most: FILES_LIMIT }));
        }
        if files_done > files {
            return Err(Refused::at("files_done", Rule::Progress));
        }
        Ok(Status {
            state,
            files_done,
            files,
        })
    }

    /// Reads these bytes as a status record, as an installation's record is read:
    /// the length first, then the bytes whole for a secret, then strictly, and last
    /// held to the one form whizbang writes.
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
        let files_done = top.whole("files_done")?;
        let files = top.whole("files")?;
        top.done()?;
        let status = Status::new(state, files_done, files)?;
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
            files_done: self.files_done,
            files: self.files,
        };
        // Writing two texts and two whole numbers as JSON cannot fail.
        serde_json::to_vec(&written).unwrap_or_default()
    }

    pub fn state(&self) -> State {
        self.state
    }

    /// How many of the entry's files were staged and held to their size and hash.
    pub fn files_done(&self) -> u64 {
        self.files_done
    }

    /// How many files the entry has.
    pub fn files(&self) -> u64 {
        self.files
    }
}

/// The model's status record: none when no install of the model ever wrote one. A
/// record that is not a regular file under its own name, is over its bound or is
/// not in the one form is refused, never followed and never guessed at.
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
