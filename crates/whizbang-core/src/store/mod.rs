//! The store: the one owner of everything durable (`SPEC.md`, "Storage").
//!
//! This module is publication and recovery. Locking, deletion and the kinds of reference
//! come with the next task of the plan; what a name means comes with artifacts and runs.
//!
//! # On disk
//!
//! ```text
//! <root>/
//!   CURRENT               the pointer: the current generation's number and a newline
//!   generations/<n>.json  generation n: every name it holds and the digest of its content
//!   objects/<sha-256>     stored content, each file named by the digest of its bytes
//!   staging/<token>/      the files of one publication on their way in
//! ```
//!
//! Every file is written in staging, with its content synced through the handle that wrote
//! it, and arrives under `objects/`, under `generations/` or at `CURRENT` by a rename. None
//! is written again. Staging is inside the root, so it is on the store's file system; a
//! staging that is not (a mount point, a link) is refused at the first rename, with
//! [`Error::OtherFileSystem`], before anything is visible.
//!
//! # What a store is
//!
//! A directory with a pointer. The pointer exists from the moment a store is created,
//! holding 0: the empty generation, which has no file.
//!
//! A directory without a pointer is not a store, whatever else it holds, and is refused
//! with [`Error::NotAStore`]; a path with nothing at it, with [`Error::Missing`]. A writer
//! makes a store in two places only: where there is nothing, and in a directory that
//! holds nothing but what an unfinished creation leaves. That is exactly: any of the
//! directories `objects/`, `generations/` and `staging/`, the first two empty, and in
//! `staging/` nothing, or one file named `CURRENT` (the first pointer, on its way). An
//! empty directory is the least of these. To a reader an unfinished creation is not a
//! store yet, which loses nothing: nothing was ever published in it.
//!
//! # Publication
//!
//! 1. Each new payload is written to staging and synced, named by its digest
//!    ([`Publication::put`]). Content the store already holds is not written again.
//! 2. The payloads are renamed into `objects/`, and `objects/` is synced. It is synced by
//!    every publication that names content, whether or not it staged any: content that was
//!    already there may have arrived by a rename that no sync has made durable yet.
//! 3. The new generation (the one current at this step with this publication's names
//!    applied, numbered one higher) is written to staging and synced, renamed into
//!    `generations/`, and `generations/` is synced. From here to the end, one publication
//!    of a process runs at a time.
//! 4. The new pointer is written to staging and synced, and renamed over `CURRENT`.
//! 5. The root is synced.
//!
//! There are two guarantees, and they are reached at different steps:
//!
//! - **Visibility** is step 4. From the rename on, a reader sees the new generation, and
//!   everything it names, content and names both, was made durable before the pointer was
//!   written.
//! - **Durability** is step 5. [`Publication::publish`] reports success only after it. A
//!   failure of step 5 itself is [`Error::Unacknowledged`]: visible, and not known to be
//!   durable.
//!
//! # Recovery
//!
//! Opening to write ([`Store::open`]) reads before it changes anything: whether there is
//! a store, a place for one, or neither, and, for a store, its pointer and its current
//! generation. A directory that is not a store, and a store that is damaged, are refused
//! there, with nothing created, removed, synced or changed. Only then does it create what
//! is missing (the root and its parents, the three directories, and for a new store the
//! pointer), and remove whatever is in staging and any generation file numbered above the
//! pointer: both are left by a publication that did not finish. Opening to read
//! ([`Reader::open`]) removes and creates nothing.
//!
//! One thing cannot be told apart from a crash, and is accepted: a pointer rewritten from
//! outside to a lower number that still has its file. A writer then takes the generation
//! files above it for leftovers and removes them.
//!
//! After a crash before success was reported the store holds the old generation or the new
//! one, whole: the pointer is replaced in one rename, and nothing it can name is incomplete.
//! After a crash once success was reported it holds the new one. Stored content that no
//! generation names can remain under `objects/`; it is complete, and it is harmless.

mod digest;
mod error;
pub mod fs;
mod generation;

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub use digest::Digest;
pub use error::Error;
pub use generation::{Entry, GENERATION_FORMAT, Generation};

use fs::Fs;

/// The store's result.
pub type Result<T> = std::result::Result<T, Error>;

/// Where the store's files are.
#[derive(Debug, Clone)]
struct Layout {
    root: PathBuf,
}

impl Layout {
    fn pointer(&self) -> PathBuf {
        self.root.join("CURRENT")
    }
    fn generations(&self) -> PathBuf {
        self.root.join("generations")
    }
    fn generation(&self, number: u64) -> PathBuf {
        self.generations().join(format!("{number}.json"))
    }
    fn objects(&self) -> PathBuf {
        self.root.join("objects")
    }
    fn object(&self, digest: &Digest) -> PathBuf {
        self.objects().join(digest.to_string())
    }
    fn staging(&self) -> PathBuf {
        self.root.join("staging")
    }
}

/// The files of a store and the file system they are on: what reading needs.
#[derive(Clone)]
struct Files {
    fs: Arc<dyn Fs>,
    layout: Layout,
}

impl Files {
    fn exists(&self, path: &Path) -> Result<bool> {
        self.fs.exists(path).map_err(Error::io("look for", path))
    }

    /// The number the pointer holds, or `None` when there is no pointer.
    fn pointer(&self) -> Result<Option<u64>> {
        let path = self.layout.pointer();
        if !self.exists(&path)? {
            return Ok(None);
        }
        let bytes = self.fs.read(&path).map_err(Error::io("read", &path))?;
        // One way to write each number: no sign, no leading zero, one newline.
        std::str::from_utf8(&bytes)
            .ok()
            .and_then(|text| text.strip_suffix('\n'))
            .and_then(|digits| {
                let number = digits.parse::<u64>().ok()?;
                (number.to_string() == digits).then_some(number)
            })
            .map(Some)
            .ok_or_else(|| Error::corrupt(&path, "not a generation number and a newline"))
    }

    /// The current generation.
    fn snapshot(&self) -> Result<Generation> {
        let number = self.pointer()?.ok_or_else(|| Error::NotAStore {
            root: self.layout.root.clone(),
        })?;
        if number == 0 {
            return Ok(Generation::empty());
        }
        let path = self.layout.generation(number);
        if !self.exists(&path)? {
            return Err(Error::corrupt(
                &self.layout.pointer(),
                format!("it names generation {number}, which has no file"),
            ));
        }
        let bytes = self.fs.read(&path).map_err(Error::io("read", &path))?;
        let generation: Generation =
            serde_json::from_slice(&bytes).map_err(|e| Error::corrupt(&path, e.to_string()))?;
        if generation.format != GENERATION_FORMAT {
            return Err(Error::corrupt(
                &path,
                format!("format {}, not {GENERATION_FORMAT}", generation.format),
            ));
        }
        if generation.number != number {
            return Err(Error::corrupt(
                &path,
                format!("it holds generation {}", generation.number),
            ));
        }
        Ok(generation)
    }

    fn read(&self, entry: &Entry) -> Result<Vec<u8>> {
        let digest = entry.digest;
        let path = self.layout.object(&digest);
        let bytes = self.fs.read(&path).map_err(|source| {
            if source.kind() == io::ErrorKind::NotFound {
                Error::corrupt(&path, format!("the content {digest} is missing"))
            } else {
                Error::io("read", &path)(source)
            }
        })?;
        if bytes.len() as u64 != entry.bytes {
            return Err(Error::corrupt(
                &path,
                format!(
                    "the content {digest} is {} bytes here, and its entry says {}",
                    bytes.len(),
                    entry.bytes
                ),
            ));
        }
        let found = Digest::of(&bytes);
        if found != digest {
            return Err(Error::corrupt(
                &path,
                format!("the content {digest} has the digest {found} here"),
            ));
        }
        Ok(bytes)
    }

    /// For a directory with no pointer: whether it holds nothing but what an unfinished
    /// creation leaves (the module's description says exactly what), so that a store may
    /// be made in it. Reads only.
    fn is_unfinished_creation(&self) -> Result<bool> {
        let layout = &self.layout;
        let list = |dir: &Path| self.fs.list(dir).map_err(Error::io("list", dir));
        let (objects, generations, staging) =
            (layout.objects(), layout.generations(), layout.staging());
        for entry in list(&layout.root)? {
            if ![&objects, &generations, &staging].contains(&&entry) {
                return Ok(false);
            }
            // Each must be a directory: listing a file fails.
            let Ok(inside) = self.fs.list(&entry) else {
                return Ok(false);
            };
            let allowed = if entry == staging {
                let pointer = staging.join("CURRENT");
                inside.is_empty()
                    || (inside == [pointer.clone()] && self.fs.list(&pointer).is_err())
            } else {
                inside.is_empty()
            };
            if !allowed {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Renames a staged file to its place.
    fn move_into_place(&self, staged: &Path, to: &Path) -> Result<()> {
        self.fs.rename(staged, to).map_err(|source| {
            if source.kind() == io::ErrorKind::CrossesDevices {
                Error::OtherFileSystem {
                    staging: self.layout.staging(),
                    source,
                }
            } else {
                Error::io("move into place", staged)(source)
            }
        })
    }
}

/// A store opened to read. It changes nothing on disk.
pub struct Reader {
    files: Files,
}

impl Reader {
    /// Opens the store at `root` to read. A path with nothing at it, and a directory that
    /// has no pointer, are refused; nothing is created, and nothing a writer left behind
    /// is touched.
    pub fn open(root: &Path, fs: Arc<dyn Fs>) -> Result<Reader> {
        let files = Files {
            fs,
            layout: Layout {
                root: root.to_path_buf(),
            },
        };
        if !files.exists(root)? {
            return Err(Error::Missing {
                root: root.to_path_buf(),
            });
        }
        if !files.exists(&files.layout.pointer())? {
            return Err(Error::NotAStore {
                root: root.to_path_buf(),
            });
        }
        Ok(Reader { files })
    }

    /// The current generation: the one the pointer names now.
    pub fn snapshot(&self) -> Result<Generation> {
        self.files.snapshot()
    }

    /// The content of an entry, checked against its length and its digest.
    pub fn read(&self, entry: &Entry) -> Result<Vec<u8>> {
        self.files.read(entry)
    }
}

/// A store opened to write.
pub struct Store {
    files: Files,
    next_token: AtomicU64,
    /// Held from the reading of the current generation to the end of a publication, so
    /// that two publications of one process never build on the same generation.
    publishing: Mutex<()>,
}

impl Store {
    /// Opens the store at `root` to write, and recovers it: leftover staging and any
    /// generation file above the pointer are removed. A store is created where there is
    /// nothing, or nothing but an unfinished creation. Everything is read first: an open
    /// that is refused has changed nothing.
    pub fn open(root: &Path, fs: Arc<dyn Fs>) -> Result<Store> {
        let files = Files {
            fs,
            layout: Layout {
                root: root.to_path_buf(),
            },
        };
        let fs = files.fs.as_ref();
        let layout = &files.layout;

        // Reading only, down to the next comment.
        let current = if !files.exists(root)? {
            0
        } else if files.exists(&layout.pointer())? {
            files.snapshot()?.number()
        } else if files.is_unfinished_creation()? {
            0
        } else {
            return Err(Error::NotAStore {
                root: root.to_path_buf(),
            });
        };

        // From here on the disk may change.
        create_durably(fs, root)?;
        let mut created = false;
        for dir in [layout.objects(), layout.generations(), layout.staging()] {
            if !files.exists(&dir)? {
                fs.create_dir(&dir).map_err(Error::io("create", &dir))?;
                created = true;
            }
        }
        if created {
            fs.sync_dir(root).map_err(Error::io("sync", root))?;
        }
        let staging = layout.staging();
        for leftover in fs.list(&staging).map_err(Error::io("list", &staging))? {
            fs.remove_all(&leftover)
                .map_err(Error::io("remove", &leftover))?;
        }
        let generations = layout.generations();
        for file in fs
            .list(&generations)
            .map_err(Error::io("list", &generations))?
        {
            let number = file
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_suffix(".json"))
                .and_then(|digits| digits.parse::<u64>().ok());
            if number.is_some_and(|number| number > current) {
                fs.remove_all(&file).map_err(Error::io("remove", &file))?;
            }
        }

        // A new store gets its pointer here, before anything can be published: from now
        // on this directory is a store.
        if files.pointer()?.is_none() {
            let staged = staging.join("CURRENT");
            fs.write_new(&staged, b"0\n")
                .map_err(Error::io("write", &staged))?;
            files.move_into_place(&staged, &layout.pointer())?;
            fs.sync_dir(root).map_err(Error::io("sync", root))?;
        }
        Ok(Store {
            files,
            next_token: AtomicU64::new(0),
            publishing: Mutex::new(()),
        })
    }

    /// The current generation.
    pub fn snapshot(&self) -> Result<Generation> {
        self.files.snapshot()
    }

    /// The content of an entry, checked against its length and its digest.
    pub fn read(&self, entry: &Entry) -> Result<Vec<u8>> {
        self.files.read(entry)
    }

    /// Starts a publication: a staging directory of its own, and nothing visible until
    /// [`Publication::publish`].
    pub fn begin(&self) -> Result<Publication<'_>> {
        let token = format!(
            "{}-{}",
            std::process::id(),
            self.next_token.fetch_add(1, Ordering::Relaxed)
        );
        let dir = self.files.layout.staging().join(token);
        self.files
            .fs
            .create_dir(&dir)
            .map_err(Error::io("create", &dir))?;
        Ok(Publication {
            files: &self.files,
            publishing: &self.publishing,
            dir,
            staged: BTreeSet::new(),
            names: BTreeMap::new(),
        })
    }
}

/// Creates a directory and the parents it lacks, each made durable in its own parent.
fn create_durably(fs: &dyn Fs, dir: &Path) -> Result<()> {
    if fs.exists(dir).map_err(Error::io("look for", dir))? {
        return Ok(());
    }
    let parent = match dir.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    create_durably(fs, parent)?;
    fs.create_dir(dir).map_err(Error::io("create", dir))?;
    fs.sync_dir(parent).map_err(Error::io("sync", parent))
}

/// A publication that succeeded: the generation is visible and durable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Published {
    /// The number of the new generation.
    pub generation: u64,
}

/// One publication in progress: names and their content, staged, to become the next
/// generation all at once. Dropping it without publishing discards it.
pub struct Publication<'s> {
    files: &'s Files,
    publishing: &'s Mutex<()>,
    dir: PathBuf,
    /// The payloads this publication wrote to staging.
    staged: BTreeSet<Digest>,
    /// The names this publication sets.
    names: BTreeMap<String, Entry>,
}

impl Publication<'_> {
    /// Gives `name` this content in the generation to come, replacing what it had. The
    /// bytes are staged now, and synced, unless the store already holds them. After an
    /// error the same call can be made again.
    pub fn put(&mut self, name: &str, bytes: &[u8]) -> Result<Digest> {
        let fs = self.files.fs.as_ref();
        let digest = Digest::of(bytes);
        if !self.staged.contains(&digest)
            && !self.files.exists(&self.files.layout.object(&digest))?
        {
            let staged = self.staged_path(&digest);
            // A write that failed part-way can have left the file behind.
            if self.files.exists(&staged)? {
                fs.remove_all(&staged)
                    .map_err(Error::io("remove", &staged))?;
            }
            fs.write_new(&staged, bytes)
                .map_err(Error::io("write", &staged))?;
            self.staged.insert(digest);
        }
        self.names.insert(
            name.to_string(),
            Entry {
                digest,
                bytes: bytes.len() as u64,
            },
        );
        Ok(digest)
    }

    fn staged_path(&self, digest: &Digest) -> PathBuf {
        self.dir.join(digest.to_string())
    }

    /// Publishes: the steps and the two guarantees are in the module's description. `Ok`
    /// means the new generation is durable. An error other than [`Error::Unacknowledged`]
    /// means it never became visible.
    pub fn publish(self) -> Result<Published> {
        let files = self.files;
        let fs = files.fs.as_ref();
        let layout = &files.layout;

        // The payloads, whose content `put` synced: their names, then the directory that
        // holds the names. The directory is synced for content that was already there
        // too: this publication is about to name it, and cannot know that the rename
        // which brought it was ever made durable.
        for digest in &self.staged {
            files.move_into_place(&self.staged_path(digest), &layout.object(digest))?;
        }
        if !self.names.is_empty() {
            let objects = layout.objects();
            fs.sync_dir(&objects).map_err(Error::io("sync", &objects))?;
        }

        // The generation, immutable from its rename on. It is built on the generation that
        // is current now, not the one that was when this publication began.
        let _publishing = self.publishing.lock().unwrap_or_else(|e| e.into_inner());
        let mut generation = files.snapshot()?;
        let number = generation.number.checked_add(1).ok_or_else(|| {
            Error::corrupt(
                &layout.pointer(),
                format!("no generation number is left after {}", generation.number),
            )
        })?;
        generation.number = number;
        generation.entries.extend(self.names.clone());
        let mut text = serde_json::to_vec_pretty(&generation)
            .map_err(|e| Error::corrupt(&layout.generation(number), e.to_string()))?;
        text.push(b'\n');
        let staged = self.dir.join("generation.json");
        fs.write_new(&staged, &text)
            .map_err(Error::io("write", &staged))?;
        files.move_into_place(&staged, &layout.generation(number))?;
        let generations = layout.generations();
        fs.sync_dir(&generations)
            .map_err(Error::io("sync", &generations))?;

        // The pointer. Its rename is visibility; the sync of its directory is durability.
        let staged = self.dir.join("CURRENT");
        fs.write_new(&staged, format!("{number}\n").as_bytes())
            .map_err(Error::io("write", &staged))?;
        files.move_into_place(&staged, &layout.pointer())?;
        fs.sync_dir(&layout.root)
            .map_err(Error::io("sync", &layout.root))
            .map_err(|source| Error::Unacknowledged {
                generation: number,
                source: Box::new(source),
            })?;
        Ok(Published { generation: number })
    }
}

impl Drop for Publication<'_> {
    /// Staging has nothing of value once the publication is over, however it ended. A
    /// failure here changes no outcome: the next open to write removes what is left.
    fn drop(&mut self) {
        let _ = self.files.fs.remove_all(&self.dir);
    }
}
