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
    /// The store's folder is another user's: it is refused when it is opened, to read
    /// or to write, before anything in it is read or locked (plan m5, task H1). Nothing
    /// was changed.
    NotOwn { root: PathBuf },
    /// The store's folder is this user's, and its mode lets its group or anyone write
    /// it: refused as [`Error::NotOwn`] is. Its mode is not changed.
    OthersMayWrite { root: PathBuf },
    /// The writer's `LOCK` is no longer the file at that name: it was removed or
    /// replaced while this writer held it, so another writer may be admitted beside it.
    /// Nothing was published, and nothing more will be by this writer (plan m5, task H1).
    LockReplaced { root: PathBuf },
    /// The store's folder is on a file system of a network type, on which its locks may
    /// hold on one machine alone: refused when it is opened, before its lock is asked
    /// (plan m6, task S1, item 4). `kind` is the type the system's table of mounts gives.
    NetworkFileSystem { root: PathBuf, kind: String },
    /// The system's table of mounts could not be read, so the file system the store is
    /// on is not known: refused to a writer, saying so. A reader is not refused.
    MountsUnknown { root: PathBuf, why: String },
    /// The writer's lock was granted to a second opening of `LOCK` while this writer
    /// held it: on this file system the locks do not hold, and a second writer would be
    /// admitted beside the first. Refused, with nothing changed.
    LocksDoNotHold { root: PathBuf },
    /// This writer keeps as many runs abandoned as it may ([`super::ABANDONED_LIMIT`]):
    /// their ends, or their admissions, became visible and were not confirmed. It admits
    /// no more runs; the process is to be started again, and the next open of the store
    /// ends them.
    TooManyAbandoned { abandoned: usize },
    /// This cannot be the id of a run: it is empty, or it holds a control character.
    InvalidRun { run: String },
    /// A generation file says it is of a format this build does not read: this build
    /// reads [`super::WHOLE_FORMAT`] and `reads`, the format it writes. A store written by
    /// other code is not migrated.
    OtherFormat {
        path: PathBuf,
        format: u32,
        reads: u32,
    },
    /// No run has this id in the current generation. Nothing was published.
    NoSuchRun { run: String },
    /// No bundle's or example's entry of this name is in the current generation, or the
    /// text is no such entry's name ([`super::Publication::let_go_of_entry`]). Nothing
    /// was published.
    NoSuchEntry { entry: String },
    /// A publication was to make this run, and the current generation holds it already.
    /// Nothing was published: two runs are never merged under one id.
    RunExists { run: String },
    /// A publication was to make this name, which belongs to no run, and the name is
    /// held already: by the current generation, or by the same publication. Nothing was
    /// published: a name that is made keeps the content it was made with.
    NameExists { name: String },
    /// A publication was to give content to a name under a prefix whose names are
    /// made and never put ([`super::MADE_PREFIXES`]): the records of request ids and
    /// of spend. Such a name is made once, by [`super::Publication::create`], and
    /// keeps its content; giving it content any other way is refused here, before
    /// anything is staged.
    MadeOnly { name: String },
    /// The reference is a diagnostic. It can be read as one, and never used as an output.
    NotAnOutput { digest: Digest },
    /// A file of the store does not hold what it must.
    Corrupt { path: PathBuf, reason: String },
    /// A failure of the store, in the words that hold no path
    /// ([`Error::without_paths`]) and with nothing else of it: what the store in an
    /// action's context answers with ([`Error::pathless`]). Whatever an action does
    /// with such a failure, as its text, as `{:?}` or as the source of another error,
    /// it writes nothing of where the store is into its run's record.
    WithoutPaths { said: String },
    /// A publication's generation file would be `bytes` long, and one is at most `limit`
    /// ([`super::GENERATION_MAX_BYTES`]): the store does not write what it will not
    /// read. Nothing was published.
    GenerationTooLarge { bytes: u64, limit: u64 },
    /// Staging is not on the store's file system, so nothing can be renamed into place.
    /// Nothing was published.
    OtherFileSystem { staging: PathBuf, source: io::Error },
    /// The pointer was replaced, so the generation is visible, but the sync that makes it
    /// durable did not succeed. It may or may not survive a crash. This is not a success,
    /// and it is not a publication that did not happen either.
    Unacknowledged { generation: u64, source: Box<Error> },
}

/// The formats a build that writes `reads` reads: that one, and the format before
/// buckets when it is later (ADR 0047).
fn reads_formats(reads: u32) -> String {
    match reads > super::WHOLE_FORMAT {
        true => format!("formats {} and {reads}", super::WHOLE_FORMAT),
        false => format!("format {reads}"),
    }
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
    /// The same failure with every path taken out of it: its words without paths, and
    /// nothing else. It is what an action is handed.
    pub fn pathless(&self) -> Error {
        Error::WithoutPaths {
            said: self.without_paths(),
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
            Error::NetworkFileSystem { kind, .. } => format!(
                "the store is on a file system of type {kind}, a network one, where its \
                 locks may not hold"
            ),
            Error::MountsUnknown { .. } => {
                "the file system the store is on could not be told".to_owned()
            }
            Error::LocksDoNotHold { .. } => {
                "the store's file system does not hold its writer's lock".to_owned()
            }
            Error::TooManyAbandoned { .. } => self.to_string(),
            Error::NotOwn { .. } => "the store's folder is another user's".to_owned(),
            Error::OthersMayWrite { .. } => {
                "the store's folder may be written by others than its owner".to_owned()
            }
            Error::LockReplaced { .. } => {
                "the store's lock file was replaced while it was held".to_owned()
            }
            Error::InvalidRun { .. } => "the store was given a run id it does not take".to_owned(),
            Error::OtherFormat {
                path,
                format,
                reads,
            } => format!(
                "the store's file {} is of another format: {format}, and this build reads {}",
                file(path),
                reads_formats(*reads)
            ),
            Error::NoSuchRun { .. } => "the store does not hold that run".to_owned(),
            Error::NoSuchEntry { .. } => {
                "the store does not hold that bundle or example".to_owned()
            }
            Error::RunExists { .. } => "the store already holds a run of that id".to_owned(),
            Error::NameExists { .. } => {
                "the store already holds a name that is made once".to_owned()
            }
            Error::MadeOnly { .. } => {
                "the store was to replace a name of a kind that is made once and never \
                 replaced"
                    .to_owned()
            }
            Error::NotAnOutput { digest } => format!(
                "the reference to {digest} is a diagnostic: it can be read, and never used \
                 as an output"
            ),
            Error::Corrupt { path, .. } => {
                format!("the store's file {} is missing or damaged", file(path))
            }
            Error::WithoutPaths { said } => said.clone(),
            Error::GenerationTooLarge { .. } => self.to_string(),
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
            Error::NetworkFileSystem { root, kind } => write!(
                f,
                "the store at {} is on a file system of type {kind}, a network one, where \
                 its locks may hold on one machine alone; a store lives on a local file \
                 system",
                root.display()
            ),
            Error::MountsUnknown { root, why } => write!(
                f,
                "the store at {} is not opened to write: the file system it is on could \
                 not be told ({why}), and a store lives on a local file system",
                root.display()
            ),
            Error::LocksDoNotHold { root } => write!(
                f,
                "the store at {} is on a file system that granted its writer's lock twice, \
                 so its locks do not hold there; a store lives on a local file system",
                root.display()
            ),
            Error::TooManyAbandoned { abandoned } => write!(
                f,
                "the store's writes failed for {abandoned} runs whose ends or admissions \
                 this process could not confirm, and it admits no more: start it again, \
                 and the next open of the store ends them"
            ),
            Error::NotOwn { root } => write!(
                f,
                "the store at {} is another user's folder, and is not opened: use a folder \
                 of your own, or have it given to you (chown)",
                root.display()
            ),
            Error::OthersMayWrite { root } => write!(
                f,
                "the store at {} may be written by its group or by anyone, and is not \
                 opened: make it yours alone (chmod go-w)",
                root.display()
            ),
            Error::LockReplaced { root } => write!(
                f,
                "the lock file of the store at {} was removed or replaced while this \
                 writer held it, so another writer may be at work; nothing was published",
                root.display()
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
                "{} is of another format: {format}, and this build reads {}",
                path.display(),
                reads_formats(*reads)
            ),
            Error::NoSuchRun { run } => write!(f, "the store holds no run {run:?}"),
            Error::NoSuchEntry { entry } => write!(
                f,
                "the store holds no bundle or example {entry:?}: an entry is bundles/ or \
                 examples/ and one name"
            ),
            Error::RunExists { run } => write!(
                f,
                "the store already holds a run {run:?}: a new run is not merged into it"
            ),
            Error::NameExists { name } => write!(
                f,
                "the store already holds the name {name:?}: it is made once and keeps its \
                 content"
            ),
            Error::MadeOnly { name } => write!(
                f,
                "the name {name:?} is of a kind that is made once and never replaced: it \
                 is given content only by making it"
            ),
            Error::NotAnOutput { digest } => write!(
                f,
                "the reference to {digest} is a diagnostic: it can be read, and never used \
                 as an output"
            ),
            Error::Corrupt { path, reason } => {
                write!(f, "{} is damaged: {reason}", path.display())
            }
            Error::WithoutPaths { said } => f.write_str(said),
            Error::GenerationTooLarge { bytes, limit } => write!(
                f,
                "the store holds too many names for one generation: its file would be \
                 {bytes} bytes, and one is at most {limit}; nothing was published"
            ),
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
            | Error::NotOwn { .. }
            | Error::OthersMayWrite { .. }
            | Error::LockReplaced { .. }
            | Error::NetworkFileSystem { .. }
            | Error::MountsUnknown { .. }
            | Error::LocksDoNotHold { .. }
            | Error::TooManyAbandoned { .. }
            | Error::NoSuchRun { .. }
            | Error::NoSuchEntry { .. }
            | Error::RunExists { .. }
            | Error::NameExists { .. }
            | Error::MadeOnly { .. }
            | Error::InvalidRun { .. }
            | Error::OtherFormat { .. }
            | Error::WithoutPaths { .. }
            | Error::GenerationTooLarge { .. }
            | Error::NotAnOutput { .. }
            | Error::Corrupt { .. } => None,
        }
    }
}
