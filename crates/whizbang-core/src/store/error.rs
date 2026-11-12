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
    /// There is a directory here, and it is not a store: it has no pointer. To a writer,
    /// which makes a store in an empty directory and in one whose creation did not finish,
    /// it also holds `found`, the first thing met that neither of those holds. A reader
    /// looks no further than the pointer and names nothing. Nothing in the directory was
    /// created, removed or changed.
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

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io {
                doing,
                path,
                source,
            } => write!(f, "cannot {doing} {}: {source}", path.display()),
            Error::Missing { root } => write!(f, "there is no store at {}", root.display()),
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
            | Error::NotAStore { .. }
            | Error::Held { .. }
            | Error::NoSuchRun { .. }
            | Error::NotAnOutput { .. }
            | Error::Corrupt { .. } => None,
        }
    }
}
