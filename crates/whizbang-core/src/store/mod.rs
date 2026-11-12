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
//! A file under `objects/` or `generations/` arrives there by a rename, after its content
//! was synced, and is never written again. Staging is inside the root, so it is on the
//! store's file system and every move into place is a rename.
//!
//! # Publication
//!
//! 1. Each new payload is written to staging, named by its digest ([`Publication::put`]).
//! 2. The payloads are synced, renamed into `objects/`, and `objects/` is synced.
//! 3. The new generation (the one current at this step with this publication's names
//!    applied, numbered one higher) is written to staging, synced, renamed into
//!    `generations/`, and `generations/` is synced. From here to the end, one publication
//!    of a process runs at a time.
//! 4. The new pointer is written to staging, synced, and renamed over `CURRENT`.
//! 5. The root is synced.
//!
//! There are two guarantees, and they are reached at different steps:
//!
//! - **Visibility** is step 4. From the rename on, a reader sees the new generation, and
//!   everything it names was made durable before the pointer was written.
//! - **Durability** is step 5. [`Publication::publish`] reports success only after it. A
//!   failure of step 5 itself is [`Error::Unacknowledged`]: visible, and not known to be
//!   durable.
//!
//! # Recovery
//!
//! Opening to write ([`Store::open`]) removes whatever is in staging, and any generation
//! file numbered above the pointer: both are left by a publication that did not finish.
//! Opening to read ([`Reader::open`]) removes and creates nothing.
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
    /// The number the pointer holds; 0 when the store never published.
    fn pointer(&self) -> Result<u64> {
        let path = self.layout.pointer();
        if !self
            .fs
            .exists(&path)
            .map_err(Error::io("look for", &path))?
        {
            return Ok(0);
        }
        let bytes = self.fs.read(&path).map_err(Error::io("read", &path))?;
        std::str::from_utf8(&bytes)
            .ok()
            .and_then(|text| text.strip_suffix('\n'))
            .filter(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|digits| digits.parse::<u64>().ok())
            .filter(|number| *number > 0)
            .ok_or_else(|| Error::corrupt(&path, "not a generation number and a newline"))
    }

    fn snapshot(&self) -> Result<Generation> {
        let number = self.pointer()?;
        if number == 0 {
            return Ok(Generation::empty());
        }
        let path = self.layout.generation(number);
        if !self
            .fs
            .exists(&path)
            .map_err(Error::io("look for", &path))?
        {
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

    fn read(&self, digest: &Digest) -> Result<Vec<u8>> {
        let path = self.layout.object(digest);
        let bytes = self.fs.read(&path).map_err(Error::io("read", &path))?;
        let found = Digest::of(&bytes);
        if found != *digest {
            return Err(Error::corrupt(&path, format!("its digest is {found}")));
        }
        Ok(bytes)
    }
}

/// A store opened to read. It changes nothing on disk.
pub struct Reader {
    files: Files,
}

impl Reader {
    /// Opens the store at `root` to read. A store that does not exist is refused, not
    /// created, and nothing a writer left behind is touched.
    pub fn open(root: &Path, fs: Arc<dyn Fs>) -> Result<Reader> {
        if !fs.exists(root).map_err(Error::io("look for", root))? {
            return Err(Error::Missing {
                root: root.to_path_buf(),
            });
        }
        Ok(Reader {
            files: Files {
                fs,
                layout: Layout {
                    root: root.to_path_buf(),
                },
            },
        })
    }

    /// The current generation: the one the pointer names now.
    pub fn snapshot(&self) -> Result<Generation> {
        self.files.snapshot()
    }

    /// The content with this digest, checked against it.
    pub fn read(&self, digest: &Digest) -> Result<Vec<u8>> {
        self.files.read(digest)
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
    /// Opens the store at `root` to write, creating it when it is not there, and recovers
    /// it: leftover staging and any generation file above the pointer are removed.
    pub fn open(root: &Path, fs: Arc<dyn Fs>) -> Result<Store> {
        let files = Files {
            fs,
            layout: Layout {
                root: root.to_path_buf(),
            },
        };
        let fs = files.fs.as_ref();
        let layout = &files.layout;
        create_durably(fs, root)?;
        // These become durable with the first publication, whose last step syncs the root.
        for dir in [layout.objects(), layout.generations(), layout.staging()] {
            if !fs.exists(&dir).map_err(Error::io("look for", &dir))? {
                fs.create_dir(&dir).map_err(Error::io("create", &dir))?;
            }
        }

        // The store is read before anything is removed: a pointer that cannot be read is
        // reported, and nothing is tidied on a guess.
        let current = files.snapshot()?.number();
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

    /// The content with this digest, checked against it.
    pub fn read(&self, digest: &Digest) -> Result<Vec<u8>> {
        self.files.read(digest)
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
    /// bytes are staged now, unless the store already holds them.
    pub fn put(&mut self, name: &str, bytes: &[u8]) -> Result<Digest> {
        let fs = self.files.fs.as_ref();
        let digest = Digest::of(bytes);
        let object = self.files.layout.object(&digest);
        if !self.staged.contains(&digest)
            && !fs.exists(&object).map_err(Error::io("look for", &object))?
        {
            let staged = self.staged_path(&digest);
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
        let fs = self.files.fs.as_ref();
        let layout = &self.files.layout;

        // The payloads: content, then names, then the directory that holds the names.
        for digest in &self.staged {
            let staged = self.staged_path(digest);
            fs.sync_file(&staged).map_err(Error::io("sync", &staged))?;
        }
        for digest in &self.staged {
            let staged = self.staged_path(digest);
            fs.rename(&staged, &layout.object(digest))
                .map_err(Error::io("move into place", &staged))?;
        }
        if !self.staged.is_empty() {
            let objects = layout.objects();
            fs.sync_dir(&objects).map_err(Error::io("sync", &objects))?;
        }

        // The generation, immutable from its rename on. It is built on the generation that
        // is current now, not the one that was when this publication began.
        let _publishing = self.publishing.lock().unwrap_or_else(|e| e.into_inner());
        let mut generation = self.files.snapshot()?;
        generation.number += 1;
        generation.entries.extend(self.names.clone());
        let number = generation.number;
        let mut text = serde_json::to_vec_pretty(&generation)
            .map_err(|e| Error::corrupt(&layout.generation(number), e.to_string()))?;
        text.push(b'\n');
        let staged = self.dir.join("generation.json");
        fs.write_new(&staged, &text)
            .map_err(Error::io("write", &staged))?;
        fs.sync_file(&staged).map_err(Error::io("sync", &staged))?;
        fs.rename(&staged, &layout.generation(number))
            .map_err(Error::io("move into place", &staged))?;
        let generations = layout.generations();
        fs.sync_dir(&generations)
            .map_err(Error::io("sync", &generations))?;

        // The pointer. Its rename is visibility; the sync of its directory is durability.
        let staged = self.dir.join("CURRENT");
        fs.write_new(&staged, format!("{number}\n").as_bytes())
            .map_err(Error::io("write", &staged))?;
        fs.sync_file(&staged).map_err(Error::io("sync", &staged))?;
        fs.rename(&staged, &layout.pointer())
            .map_err(Error::io("move into place", &staged))?;
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
