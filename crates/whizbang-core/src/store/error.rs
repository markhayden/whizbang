//! What the store can refuse.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use super::digest::Digest;

/// A failure of the store.
#[derive(Debug)]
pub enum Error {
    /// The file system refused a call.
    Io {
        /// What the store was doing, as in "cannot <doing> <path>".
        doing: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    /// There is no store here. Opening to read creates nothing.
    Missing { root: PathBuf },
    /// There is nothing here, and the directory that would hold a store is missing too.
    /// A writer that makes the root alone ([`super::Store::open_in_existing_parent`])
    /// made nothing.
    NoParent { root: PathBuf, parent: PathBuf },
    /// There is a directory here, and it is not a store: it has no pointer. `found` is
    /// the first thing met in it that a store being created does not hold, which is why
    /// a writer does not make a store here; `None` when there is no such thing, as in an
    /// empty directory, which is not a store to a reader or to a writer that must find
    /// one. Nothing in the directory was created, removed or changed.
    NotAStore {
        root: PathBuf,
        found: Option<PathBuf>,
    },
    /// Another writer holds the store: one process at a time may write it. `holder` is
    /// what that writer wrote of itself in the lock file, when it could be read. Nothing
    /// was removed or changed; reading is not refused.
    Held {
        root: PathBuf,
        holder: Option<String>,
    },
    /// This cannot be the id of a run: it is empty, or it holds a control character.
    InvalidRun { run: String },
    /// A generation file says it is of a format this build does not read. A store written
    /// by other code is not migrated.
    OtherFormat {
        path: PathBuf,
        format: u32,
        reads: u32,
    },
    /// No run has this id in the current generation. Nothing was published.
    NoSuchRun { run: String },
    /// The reference is a diagnostic. It can be read as one, and never used as an output.
    NotAnOutput { digest: Digest },
    /// A file of the store does not hold what it must.
    Corrupt { path: PathBuf, reason: String },
    /// Staging is not on the store's file system, so nothing can be renamed into place.
    /// Nothing was published.
    OtherFileSystem { staging: PathBuf, source: io::Error },
    /// The pointer was replaced, so the generation is visible, but the sync that makes it
    /// durable did not succeed. It may or may not survive a crash. This is not a success,
    /// and it is not a publication that did not happen either.
    Unacknowledged { generation: u64, source: Box<Error> },
}

impl Error {
    pub(crate) fn io(doing: &'static str, path: &Path) -> impl FnOnce(io::Error) -> Error {
        move |source| Error::Io {
            doing,
            path: path.to_path_buf(),
            source,
        }
    }

    pub(crate) fn corrupt(path: &Path, reason: impl Into<String>) -> Error {
        Error::Corrupt {
            path: path.to_path_buf(),
            reason: reason.into(),
        }
    }
}

impl Error {
    /// The failure in words that hold no path: what may be handed to a caller who is not
    /// on this machine. A file of the store is named by its last part alone, which is a
    /// digest, a generation's number or a fixed name, and never by where the store is.
    /// The reason a file was found damaged is left out: it can quote what the file holds.
    /// [`fmt::Display`] gives the whole of it, paths included, for whoever runs the store.
    pub fn without_paths(&self) -> String {
        let file = |path: &Path| match path.file_name() {
            Some(name) => name.to_string_lossy().into_owned(),
            None => "a file".to_owned(),
        };
        match self {
            Error::Io {
                doing,
                path,
                source,
            } => format!(
                "the store cannot {doing} its file {}: {}",
                file(path),
                source.kind()
            ),
            Error::Missing { .. } | Error::NoParent { .. } => "there is no store".to_owned(),
            Error::NotAStore { .. } => "what was opened is not a store".to_owned(),
            Error::Held { .. } => "the store is being written by another process".to_owned(),
            Error::InvalidRun { .. } => "the store was given a run id it does not take".to_owned(),
            Error::OtherFormat {
                path,
                format,
                reads,
            } => format!(
                "the store's file {} is of another format: {format}, and this build reads \
                 format {reads}",
                file(path)
            ),
            Error::NoSuchRun { .. } => "the store does not hold that run".to_owned(),
            Error::NotAnOutput { digest } => format!(
                "the reference to {digest} is a diagnostic: it can be read, and never used \
                 as an output"
            ),
            Error::Corrupt { path, .. } => {
                format!("the store's file {} is missing or damaged", file(path))
            }
            Error::OtherFileSystem { .. } => {
                "the store's staging is not on the store's file system".to_owned()
            }
            Error::Unacknowledged { generation, .. } => {
                format!("generation {generation} is visible but was not confirmed durable")
            }
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io {
                doing,
                path,
                source,
            } => write!(f, "cannot {doing} {}: {source}", path.display()),
            Error::Missing { root } => write!(f, "there is no store at {}", root.display()),
            Error::NoParent { root, parent } => write!(
                f,
                "there is no store at {}, and none is made there: there is no directory \
                 {} to make it in",
                root.display(),
                parent.display()
            ),
            Error::NotAStore { root, found: None } => {
                write!(f, "{} is not a store: it has no CURRENT", root.display())
            }
            Error::NotAStore {
                root,
                found: Some(found),
            } => write!(
                f,
                "{} is not a store: it has no CURRENT, and it holds {}, which a store that \
                 is being created does not",
                root.display(),
                found.display()
            ),
            Error::Held { root, holder } => write!(
                f,
                "the store at {} is being written by {}; one process at a time may write a \
                 store",
                root.display(),
                holder.as_deref().unwrap_or("another process")
            ),
            Error::InvalidRun { run } => write!(
                f,
                "{run:?} cannot be a run's id: an id is not empty and holds no control \
                 character"
            ),
            Error::OtherFormat {
                path,
                format,
                reads,
            } => write!(
                f,
                "{} is of another format: {format}, and this build reads format {reads}",
                path.display()
            ),
            Error::NoSuchRun { run } => write!(f, "the store holds no run {run:?}"),
            Error::NotAnOutput { digest } => write!(
                f,
                "the reference to {digest} is a diagnostic: it can be read, and never used \
                 as an output"
            ),
            Error::Corrupt { path, reason } => {
                write!(f, "{} is damaged: {reason}", path.display())
            }
            Error::OtherFileSystem { staging, source } => write!(
                f,
                "{} is not on the store's file system: {source}",
                staging.display()
            ),
            Error::Unacknowledged { generation, source } => write!(
                f,
                "generation {generation} is visible but was not confirmed durable: {source}"
            ),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io { source, .. } | Error::OtherFileSystem { source, .. } => Some(source),
            Error::Unacknowledged { source, .. } => Some(source.as_ref()),
            Error::Missing { .. }
            | Error::NoParent { .. }
            | Error::NotAStore { .. }
            | Error::Held { .. }
            | Error::NoSuchRun { .. }
            | Error::InvalidRun { .. }
            | Error::OtherFormat { .. }
            | Error::NotAnOutput { .. }
            | Error::Corrupt { .. } => None,
        }
    }
}
