//! The store: the one owner of everything durable (`SPEC.md`, "Storage").
//!
//! This module is publication and recovery, the one writer, runs and their deletion, the
//! two kinds of reference, and prune. What a name means comes with artifacts and the run
//! record.
//!
//! # On disk
//!
//! ```text
//! <root>/
//!   CURRENT               the pointer: the current generation's number and a newline
//!   LOCK                  the writer lock; it holds one line saying who the writer is
//!   generations/<n>.json  generation n: every name it holds, outside runs and by run,
//!                         with the digest of its content and the kind of the reference
//!   objects/<sha-256>     stored content, each file named by the digest of its bytes
//!   staging/<token>/      the files of one publication on their way in
//! ```
//!
//! Every file is written in staging, with its content synced through the handle that wrote
//! it, and arrives under `objects/`, under `generations/` or at `CURRENT` by a rename. None
//! is written again. Staging is inside the root, so it is on the store's file system; a
//! staging that is not (a mount point, a link) is refused at the first rename, with
//! [`Error::OtherFileSystem`], before anything is visible. The lock file is the one file
//! written in place, and nothing depends on what it holds.
//!
//! The real file system creates every directory for its owner only (0700) and every file
//! for its owner only (0600): the root when the store makes it, the directories above the
//! root that it has to make, staging, stored content, generations, the pointer and the
//! lock file.
//!
//! # What a store is
//!
//! A directory with a pointer. The pointer exists from the moment a store is created,
//! holding 0: the empty generation, which has no file.
//!
//! A directory without a pointer is not a store, whatever else it holds, and is refused
//! with [`Error::NotAStore`]; a path with nothing at it, with [`Error::Missing`]. A writer
//! makes a store in two places only: where there is nothing, and in a directory that
//! holds nothing but what an unfinished creation leaves. That is exactly: the file `LOCK`;
//! any of the directories `objects/`, `generations/` and `staging/`, the first two empty;
//! and in `staging/` nothing, or one file named `CURRENT` (the first pointer, on its way)
//! that holds `0` and a newline, or the start of that when a crash cut it short. An empty
//! directory is the least of these. Anything else is refused, and the refusal names the
//! first thing found that does not belong ([`Error::NotAStore`]). To a reader an
//! unfinished creation is not a store yet, which loses nothing: nothing was ever
//! published in it.
//!
//! Where there is nothing, [`Store::open`] makes the root and every directory above it
//! that is missing. [`Store::open_in_existing_parent`] makes the root alone: where the
//! directory that would hold it is missing, it is refused with [`Error::NoParent`] and
//! nothing is made, as `mkdir` without `-p` refuses, so that a path with a mistyped
//! part does not become a store somewhere unintended. A writer knows whether its open
//! made the store ([`Store::created`]): it is the one that wrote the first pointer,
//! under the lock, whether it began from nothing or completed an unfinished creation.
//!
//! # One writer
//!
//! A writer takes the lock on `LOCK` when it opens the store and holds it for as long as
//! its [`Store`] lives; the lock ends with the process, however the process ends. A
//! second writer, in another process or in the same one, is refused at once with
//! [`Error::Held`], which names the holder as the holder wrote itself in the file: its
//! process id and its program. A reader never takes this lock and is never refused.
//!
//! A writer writes its name once it holds the lock and has found the store sound, and
//! takes it out when it is dropped. A writer that dies leaves its name in the file. So
//! for the moment between a writer's taking the lock and its writing, one that is refused
//! is told of "another process", or, after a death, the name of the dead one; nothing
//! but the message depends on the name.
//!
//! Nothing is removed, by recovery or otherwise, except by a writer that holds the lock.
//! An open that is refused, for any reason, has removed and changed nothing; only where
//! there was no directory at all has it made one, to hold the lock file. In particular a
//! writer refused for damage has not written its name.
//!
//! What a writer sees before it holds the lock can be another writer's change half made:
//! a pointer that has moved on from the generation file just looked for, a store whose
//! creation ends between two looks. So no fault seen then is reported as seen. If the
//! lock cannot be taken the answer is [`Error::Held`]; if it can, everything is read
//! again under it and that is the answer. Only a directory with no lock file in it, which
//! no writer is at work in, is refused on the first look alone, so that nothing, not even
//! a lock file, is made in a directory that is not a store.
//!
//! A writer that takes the lock to confirm a fault holds it for the moment its second
//! reading takes, and writes no name: it is about to be refused. Another writer that
//! starts in that moment is refused with [`Error::Held`], and rightly: the store is
//! held. What it is told is what the lock file holds then, which is not the name of the
//! one that holds it: "another process" when the file is empty, or the name a dead
//! writer left. The name in a refusal is for the message only, and may be stale.
//!
//! The lock is the standard library's file lock, `flock` on Linux and macOS: it works
//! on local file systems only, and `LOCK` must never be removed or replaced while a
//! writer holds it. The module [`fs`] says exactly what holds and what does not.
//!
//! A writer may start child processes while it holds a store, and after. A child being
//! started holds a copy of the open lock file until its program starts, so a writer lets
//! its lock go when its [`Store`] is dropped, and does not leave that to the closing of
//! the file: the next writer is admitted at once, whatever child another thread is
//! starting. Only the process that took the lock lets it go: a forked copy of a writer
//! that drops its copy of the [`Store`] ends nothing, and takes no name out. The limit
//! is a writer that ends without dropping its [`Store`] (killed by a signal, which is
//! how Ctrl-C ends it, aborted, or ended by `exit`) while one of its threads is starting
//! a child: until that child's program starts, a moment, the next writer is refused
//! with [`Error::Held`], and is told of "another process" or the name the dead writer
//! left; and where the writer was pruning, a reader's snapshot can wait that long, since
//! a reader waits for a generation file that is taken alone. A process that holds a
//! store and starts a worker ends by returning, with its store dropped, or accepts that.
//!
//! # Runs, and the two kinds of reference
//!
//! A generation holds names that belong to no run, and, for each run, the names of that
//! run ([`Publication::put`], [`Publication::put_in`]). A publication that makes a run
//! says so ([`Publication::create_run`]) and is refused when the store already holds a
//! run of that id; one that does not say so adds its names to the run, new or not. Each name is a reference to stored
//! content and says what kind it is ([`Kind`]): a committed output, or a diagnostic. The
//! kind is the reference's, not the content's: the same bytes can be a diagnostic to one
//! run and an output to another, are stored once, and are treated as each reference says.
//! A diagnostic is read like anything else and is refused where an output is asked for
//! ([`Store::read_output`]).
//!
//! Deleting a run ([`Store::delete`]) publishes a generation without that run's names.
//! Content that another reference still names, in another run or outside runs, is still
//! named. Nothing removes a name that belongs to no run: that is where the records that
//! must outlive every run, of spend and of request ids, are published. That is all the
//! protection there is against removal. Against replacement there is
//! [`Publication::create`]: a publication that makes a name is refused, whole, when the
//! store already holds the name, so a record that must be kept is published under a name
//! that is made, and is never given other content by code that makes its names.
//! [`Publication::put`] still replaces what a name had, and what it had is then named by
//! nothing and is pruned: no code of whizbang puts a name that another makes.
//!
//! A run that one publication makes and another adds to is added to with
//! [`Publication::extend_run`], which refuses the second publication when the run was
//! deleted in between ([`Error::NoSuchRun`]).
//!
//! A run's id is not empty and holds no control character; [`Publication::put_in`]
//! refuses any other ([`Error::InvalidRun`]). The rule is held where a run is made and,
//! deliberately, not where a generation file is read: a file that other hands wrote can
//! hold a run whose id is empty or has a control character in it, and that run is read,
//! listed and deleted like any other. Refusing it on reading would make it a run that
//! nothing can remove, in a store that nothing can read. Whoever prints an id escapes it.
//! The ids the executor gives runs keep a narrower rule ([`crate::contract::RunId`]).
//! Deleting a run the store does not hold is
//! refused ([`Error::NoSuchRun`]), and then none of the runs named with it is deleted.
//!
//! # Prune
//!
//! Deleting unnames; [`Store::prune`] removes: the files of generations older than the
//! current one, and stored content that no generation left names. Two things are left
//! though nothing current names them. A generation that a reader still holds, with all
//! it names: a reader shares the file of the generation of its latest snapshot, and prune
//! removes only a generation file that it can take alone. And content that a publication
//! in progress in this process has named. Both go at the first prune after the reader or
//! the publication has finished.
//!
//! Prune removes generation files, syncs their directory, removes content, and syncs its
//! directory. It never removes anything the current generation names, so a crash at any
//! point leaves the store whole, with at worst some of what was to go still there.
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
//! generation. Then it takes the writer lock, making the root first where there was none,
//! and reads the same things again: this second reading is the one that refuses a
//! directory that is not a store or a store that is damaged, with nothing created,
//! removed, synced or changed, and the one recovery goes by ("One writer" says why).
//! Only then does it create what is missing (the three directories, and for a new store the pointer), and
//! remove whatever is in staging and any generation file numbered above the pointer: both
//! are left by a publication that did not finish. Opening to read ([`Reader::open`])
//! removes and creates nothing.
//!
//! One thing cannot be told apart from a crash, and is accepted: a pointer rewritten from
//! outside to a lower number that still has its file. A writer then takes the generation
//! files above it for leftovers and removes them.
//!
//! After a crash before success was reported the store holds the old generation or the new
//! one, whole: the pointer is replaced in one rename, and nothing it can name is incomplete.
//! After a crash once success was reported it holds the new one. Stored content that no
//! generation names can remain under `objects/`; it is complete, it is harmless, and the
//! next prune removes it.

mod digest;
mod error;
pub mod fs;
mod generation;

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, TryLockError};

pub use digest::Digest;
pub use error::Error;
pub use generation::{Entry, GENERATION_FORMAT, Generation, Kind, Run};

use fs::{Fs, Held, LockFile};

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
        self.objects().join(digest.hex())
    }
    fn staging(&self) -> PathBuf {
        self.root.join("staging")
    }
    fn lock(&self) -> PathBuf {
        self.root.join("LOCK")
    }
}

/// What a writer's open may make where there is no store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Make {
    /// Nothing: the store must be there.
    Nothing,
    /// A store, in a root whose parent directory is there.
    RootOnly,
    /// A store, and every directory above its root that is missing.
    WithParents,
}

/// The directory that holds `dir`: for a name alone, the current directory.
fn parent_of(dir: &Path) -> &Path {
    match dir.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
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

    /// The number the pointer holds, in a directory that must be a store.
    fn current(&self) -> Result<u64> {
        self.pointer()?.ok_or_else(|| Error::NotAStore {
            root: self.layout.root.clone(),
            found: None,
        })
    }

    /// The current generation.
    fn snapshot(&self) -> Result<Generation> {
        let number = self.current()?;
        if number == 0 {
            return Ok(Generation::empty());
        }
        if !self.exists(&self.layout.generation(number))? {
            return Err(self.no_file(number));
        }
        self.generation(number)
    }

    /// The pointer names a generation that has no file.
    fn no_file(&self, number: u64) -> Error {
        Error::corrupt(
            &self.layout.pointer(),
            format!("it names generation {number}, which has no file"),
        )
    }

    /// The current generation, shared so that a prune leaves it, and everything it names,
    /// for as long as the second value is held. The empty generation names nothing and
    /// needs no holding.
    fn held_snapshot(&self) -> Result<(Generation, Option<Box<dyn Held>>)> {
        loop {
            let number = self.current()?;
            if number == 0 {
                return Ok((Generation::empty(), None));
            }
            let path = self.layout.generation(number);
            let held = match self.fs.share(&path) {
                Ok(held) => held,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    // A prune removes only generations older than the current one: if this
                    // one went since the pointer was read, the pointer has moved on.
                    if self.current()? == number {
                        return Err(self.no_file(number));
                    }
                    continue;
                }
                Err(error) => return Err(Error::io("share", &path)(error)),
            };
            // A prune that took the file alone before it was shared has removed its name
            // by now, and may go on to remove what it names. With the name still there,
            // no prune can take it while this is held.
            if !self.exists(&path)? {
                continue;
            }
            return Ok((self.generation(number)?, Some(held)));
        }
    }

    /// Generation `number`, from its file.
    fn generation(&self, number: u64) -> Result<Generation> {
        let path = self.layout.generation(number);
        let bytes = self.fs.read(&path).map_err(Error::io("read", &path))?;
        // The format first, by itself: a file of another format does not have this
        // format's shape, and is another format, not damage.
        #[derive(serde::Deserialize)]
        struct Format {
            format: u32,
        }
        if let Ok(Format { format }) = serde_json::from_slice(&bytes)
            && format != GENERATION_FORMAT
        {
            return Err(Error::OtherFormat {
                path,
                format,
                reads: GENERATION_FORMAT,
            });
        }
        let generation: Generation =
            serde_json::from_slice(&bytes).map_err(|e| Error::corrupt(&path, e.to_string()))?;
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
        let found = Digest::of_bytes(&bytes);
        if found != digest {
            return Err(Error::corrupt(
                &path,
                format!("the content {digest} has the digest {found} here"),
            ));
        }
        Ok(bytes)
    }

    fn read_output(&self, entry: &Entry) -> Result<Vec<u8>> {
        match entry.kind {
            Kind::Output => self.read(entry),
            Kind::Diagnostic => Err(Error::NotAnOutput {
                digest: entry.digest,
            }),
        }
    }

    /// What is at the root, for a writer: the number of the current generation of the
    /// store there, or, when a store may be made (`make`), 0 where one may be made:
    /// where there is nothing (and, for [`Make::RootOnly`], a directory to hold the
    /// root), or nothing but an unfinished creation. Anything else is refused. Reads
    /// only.
    ///
    /// Without the writer lock this is a look at something another writer may be changing
    /// between two of its reads, and what it reports can be wrong: damage that is not
    /// there, a directory that is not a store and is one. Only under the lock is it final.
    fn place(&self, make: Make) -> Result<u64> {
        let root = &self.layout.root;
        if !self.exists(root)? {
            return match make {
                Make::Nothing => Err(Error::Missing { root: root.clone() }),
                Make::RootOnly if !self.exists(parent_of(root))? => Err(Error::NoParent {
                    root: root.clone(),
                    parent: parent_of(root).to_path_buf(),
                }),
                Make::RootOnly | Make::WithParents => Ok(0),
            };
        }
        if self.exists(&self.layout.pointer())? {
            return Ok(self.snapshot()?.number());
        }
        match self.beyond_an_unfinished_creation()? {
            None if make != Make::Nothing => Ok(0),
            found => Err(Error::NotAStore {
                root: root.clone(),
                found,
            }),
        }
    }

    /// For a directory with no pointer: the first thing in it that an unfinished creation
    /// does not leave (the module's description says exactly what it leaves), or `None`
    /// when a store may be made here. Reads only.
    ///
    /// Another writer may be finishing this very creation meanwhile. What was listed and
    /// is gone when it is looked at is not objected to: it is not there. The writer lock,
    /// which comes after this, settles who goes on.
    fn beyond_an_unfinished_creation(&self) -> Result<Option<PathBuf>> {
        let layout = &self.layout;
        let gone = |error: &io::Error| error.kind() == io::ErrorKind::NotFound;
        // The entries of a directory; `None` for what is not a directory.
        let inside = |path: &Path| match self.fs.list(path) {
            Ok(inside) => Ok(Some(inside)),
            Err(error) if error.kind() == io::ErrorKind::NotADirectory => Ok(None),
            Err(error) if gone(&error) => Ok(Some(Vec::new())),
            Err(error) => Err(Error::io("list", path)(error)),
        };
        let (objects, generations, staging) =
            (layout.objects(), layout.generations(), layout.staging());
        for entry in self
            .fs
            .list(&layout.root)
            .map_err(Error::io("list", &layout.root))?
        {
            let inside = inside(&entry)?;
            if entry == layout.lock() {
                // The lock file: a file, whatever it holds.
                if inside.is_some() && self.exists(&entry)? {
                    return Ok(Some(entry));
                }
                continue;
            }
            let Some(inside) = inside else {
                return Ok(Some(entry));
            };
            if entry == objects || entry == generations {
                if let Some(first) = inside.into_iter().next() {
                    return Ok(Some(first));
                }
            } else if entry == staging {
                // Nothing, or the first pointer on its way: a file that holds what the
                // first pointer holds, or the start of it when a crash cut it short.
                for staged in inside {
                    if staged != staging.join("CURRENT") {
                        return Ok(Some(staged));
                    }
                    match self.fs.read(&staged) {
                        Ok(bytes) if FIRST_POINTER.starts_with(&bytes) => {}
                        Ok(_) => return Ok(Some(staged)),
                        Err(error) if error.kind() == io::ErrorKind::IsADirectory => {
                            return Ok(Some(staged));
                        }
                        // Moved into place since it was listed.
                        Err(error) if gone(&error) => {}
                        Err(error) => return Err(Error::io("read", &staged)(error)),
                    }
                }
            } else {
                return Ok(Some(entry));
            }
        }
        Ok(None)
    }

    /// Who holds the writer lock, as the holder wrote it: one line, kept short, and
    /// nothing that is not printable. `None` when it cannot be read.
    fn holder(&self) -> Option<String> {
        let bytes = self.fs.read(&self.layout.lock()).ok()?;
        let text = String::from_utf8_lossy(&bytes);
        let line: String = text
            .lines()
            .next()?
            .chars()
            .filter(|c| !c.is_control())
            .take(200)
            .collect();
        (!line.trim().is_empty()).then(|| line.trim().to_string())
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

/// A store opened to read. It changes nothing on disk, and takes no part in the writer
/// lock: it reads while a writer holds the store.
///
/// A reader keeps the generation of its latest [`Reader::snapshot`] safe from a prune,
/// with everything that generation names, until its next snapshot or until it is dropped:
/// it shares the generation's file, and a prune removes only what nobody shares.
pub struct Reader {
    files: Files,
    /// The share of the generation of the latest snapshot.
    held: Mutex<Option<Box<dyn Held>>>,
}

impl Reader {
    /// Opens the store at `root` to read. A path with nothing at it, and a directory that
    /// has no pointer, are refused; nothing is created, and nothing a writer left behind
    /// is touched. The refusal of a directory names what was found in it that a store
    /// being created does not hold, if anything was ([`Error::NotAStore`]).
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
            // Not a store. What else is here says why, when it can be read. A writer may
            // be finishing the creation of this very store: if the pointer is there after
            // the look, what the look found is no longer the answer.
            let found = files.beyond_an_unfinished_creation().ok().flatten();
            if !files.exists(&files.layout.pointer())? {
                return Err(Error::NotAStore {
                    root: root.to_path_buf(),
                    found,
                });
            }
        }
        Ok(Reader {
            files,
            held: Mutex::new(None),
        })
    }

    /// The current generation: the one the pointer names now. From here on this is the
    /// generation the reader keeps safe from a prune, in place of the one before.
    pub fn snapshot(&self) -> Result<Generation> {
        let (generation, held) = self.files.held_snapshot()?;
        *self.held.lock().unwrap_or_else(|e| e.into_inner()) = held;
        Ok(generation)
    }

    /// The content of an entry of either kind, checked against its length and its digest.
    pub fn read(&self, entry: &Entry) -> Result<Vec<u8>> {
        self.files.read(entry)
    }

    /// The content of an entry, to be used as an output: a diagnostic is refused with
    /// [`Error::NotAnOutput`], whatever other reference the same bytes have.
    pub fn read_output(&self, entry: &Entry) -> Result<Vec<u8>> {
        self.files.read_output(entry)
    }
}

/// A store opened to write.
pub struct Store {
    files: Files,
    next_token: AtomicU64,
    /// Held from the reading of the current generation to the end of a publication, so
    /// that two publications of one process never build on the same generation.
    publishing: Mutex<()>,
    /// The content each publication in progress has named, by the publication's number: a
    /// prune keeps it, though no generation names it yet. A prune holds this while it
    /// removes content, and a publication takes it before it looks for content.
    in_progress: Mutex<BTreeMap<u64, BTreeSet<Digest>>>,
    /// How many publications are waiting for `publishing` now.
    waiting: AtomicUsize,
    /// The runs this value made: each by a publication that became visible.
    made_runs: Mutex<BTreeSet<String>>,
    /// Whether the open that gave this value made the store.
    created: bool,
    /// The writer lock, held for as long as this value lives.
    lock: Box<dyn LockFile>,
}

impl Drop for Store {
    /// The lock ends with this value. The name in the lock file is taken out first, so
    /// that it does not outlive its holder; a writer that dies leaves its name behind.
    fn drop(&mut self) {
        let _ = self.lock.record(b"");
    }
}

/// A run's id: not empty, and no control character (a line break, a tab, an escape). This
/// is the floor. The executor names a run with a [`crate::contract::RunId`], whose rule is
/// narrower, and nothing else makes a run.
fn check_run_id(run: &str) -> Result<()> {
    if run.is_empty() || run.chars().any(char::is_control) {
        return Err(Error::InvalidRun {
            run: run.to_string(),
        });
    }
    Ok(())
}

/// What the first pointer of a store holds.
const FIRST_POINTER: &[u8] = b"0\n";

/// What a writer writes in the lock file, for a writer that is refused to show: its
/// process id and the name of its program.
fn holder() -> String {
    let program = std::env::args_os()
        .next()
        .and_then(|path| {
            Path::new(&path)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "a program without a name".to_string());
    format!("process {} ({program})\n", std::process::id())
}

impl Store {
    /// Opens the store at `root` to write, takes the writer lock, and recovers it:
    /// leftover staging and any generation file above the pointer are removed. A store is
    /// created where there is nothing, or nothing but an unfinished creation.
    ///
    /// An open that is refused has removed and changed nothing, whatever the reason: this
    /// is not a store ([`Error::NotAStore`]), it is damaged, or another writer holds it
    /// ([`Error::Held`]). What is reported is what the store is with no other writer at
    /// work on it: damage and "not a store" are found, or confirmed, under the lock, and
    /// while another writer holds the lock the answer is [`Error::Held`].
    pub fn open(root: &Path, fs: Arc<dyn Fs>) -> Result<Store> {
        Store::open_at(root, fs, Make::WithParents)
    }

    /// As [`Store::open`], except that where there is nothing at `root` only the root
    /// itself is made: a missing parent directory is refused with [`Error::NoParent`],
    /// and nothing is made, as `mkdir` without `-p` refuses. For a path a person typed.
    pub fn open_in_existing_parent(root: &Path, fs: Arc<dyn Fs>) -> Result<Store> {
        Store::open_at(root, fs, Make::RootOnly)
    }

    /// As [`Store::open`], for a store that must be there already: nothing is created
    /// where there is none ([`Error::Missing`]), and an empty directory or an unfinished
    /// creation is not completed ([`Error::NotAStore`]). For what removes things.
    pub fn open_existing(root: &Path, fs: Arc<dyn Fs>) -> Result<Store> {
        Store::open_at(root, fs, Make::Nothing)
    }

    /// Whether the open that gave this value made the store: no store was here when it
    /// took the writer lock, and it wrote the first pointer. That is so where there was
    /// nothing, in an empty directory, and where another writer's creation was left
    /// unfinished and this one completed it. It is decided under the lock, so nothing
    /// another process does between two looks can make it wrong.
    pub fn created(&self) -> bool {
        self.created
    }

    /// For tests: how many publications of this value have come to the step that one
    /// publication runs at a time, found it taken by a prune or by another publication,
    /// and are waiting there. One that found it free was never counted. One that is
    /// counted has made nothing visible yet.
    #[doc(hidden)]
    pub fn publications_waiting(&self) -> usize {
        self.waiting.load(Ordering::SeqCst)
    }

    /// Whether this writer made the run `run` since it opened the store: by a
    /// publication of this value that made the run ([`Publication::create_run`]) and
    /// became visible. A run the store held when it was opened, one that was only added
    /// to, and one whose publication was refused, were not made here. One process at a
    /// time writes a store, so a run that this writer did not make and that is not
    /// finished is no longer being worked on by anyone.
    pub fn made_here(&self, run: &str) -> bool {
        self.made_runs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(run)
    }

    fn open_at(root: &Path, fs: Arc<dyn Fs>, make: Make) -> Result<Store> {
        let files = Files {
            fs,
            layout: Layout {
                root: root.to_path_buf(),
            },
        };
        let fs = files.fs.as_ref();
        let layout = &files.layout;
        let lock_path = layout.lock();

        // A look before the lock, reading only, for one purpose: a place where no store is
        // or may be made is refused with nothing made in it, not even a lock file.
        let lock = match files.place(make) {
            Ok(_) => {
                // The lock file is in the root, so the root comes first. It is made only
                // where there was nothing.
                create_durably(fs, root, make == Make::WithParents)?;
                fs.hold_lock_file(&lock_path, true)
                    .map_err(Error::io("lock", &lock_path))?
            }
            // The look found fault. Another writer may have been changing the store
            // between two reads of it, so the fault is not reported yet. With no lock file
            // there is no such writer, and it stands. With one, the lock decides.
            Err(seen) => match fs.hold_lock_file(&lock_path, false) {
                Ok(lock) => lock,
                Err(_) => return Err(seen),
            },
        };
        let Some(lock) = lock else {
            return Err(Error::Held {
                root: root.to_path_buf(),
                holder: files.holder(),
            });
        };

        // The lock is held: no other writer is at work, and none can start. Everything is
        // read again, and this reading is the one that counts: a refusal here is of the
        // store as it is, and recovery goes by this and not by the look before the lock:
        // since that look another writer may have published. Until here nothing was
        // changed.
        let current = files.place(make)?;
        lock.record(holder().as_bytes())
            .map_err(Error::io("write", &lock_path))?;

        // From here on the disk may change.
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
            let number = generation_number(&file);
            if number.is_some_and(|number| number > current) {
                fs.remove_all(&file).map_err(Error::io("remove", &file))?;
            }
        }

        // A new store gets its pointer here, before anything can be published: from now
        // on this directory is a store.
        let created = files.pointer()?.is_none();
        if created {
            let staged = staging.join("CURRENT");
            fs.write_new(&staged, FIRST_POINTER)
                .map_err(Error::io("write", &staged))?;
            files.move_into_place(&staged, &layout.pointer())?;
            fs.sync_dir(root).map_err(Error::io("sync", root))?;
        }
        Ok(Store {
            files,
            next_token: AtomicU64::new(0),
            publishing: Mutex::new(()),
            in_progress: Mutex::new(BTreeMap::new()),
            waiting: AtomicUsize::new(0),
            made_runs: Mutex::new(BTreeSet::new()),
            created,
            lock,
        })
    }

    /// The current generation.
    pub fn snapshot(&self) -> Result<Generation> {
        self.files.snapshot()
    }

    /// The content of an entry of either kind, checked against its length and its digest.
    pub fn read(&self, entry: &Entry) -> Result<Vec<u8>> {
        self.files.read(entry)
    }

    /// The content of an entry, to be used as an output: a diagnostic is refused with
    /// [`Error::NotAnOutput`], whatever other reference the same bytes have.
    pub fn read_output(&self, entry: &Entry) -> Result<Vec<u8>> {
        self.files.read_output(entry)
    }

    /// Starts a publication: a staging directory of its own, and nothing visible until
    /// [`Publication::publish`].
    pub fn begin(&self) -> Result<Publication<'_>> {
        let number = self.next_token.fetch_add(1, Ordering::Relaxed);
        let dir = self
            .files
            .layout
            .staging()
            .join(format!("{}-{number}", std::process::id()));
        self.files
            .fs
            .create_dir(&dir)
            .map_err(Error::io("create", &dir))?;
        Ok(Publication {
            store: self,
            files: &self.files,
            number,
            dir,
            staged: BTreeSet::new(),
            names: BTreeMap::new(),
            run_names: BTreeMap::new(),
            deleted: BTreeSet::new(),
            created: BTreeSet::new(),
            made: BTreeSet::new(),
            extended: BTreeSet::new(),
        })
    }

    /// Deletes runs: publishes a generation without them, in which nothing they alone
    /// referenced is named any more. Names that belong to no run are not touched. A run
    /// the store does not hold is refused with [`Error::NoSuchRun`], and nothing is
    /// published. The content itself stays on disk until [`Store::prune`].
    pub fn delete(&self, runs: &[&str]) -> Result<Published> {
        let mut publication = self.begin()?;
        publication.deleted = runs.iter().map(|run| run.to_string()).collect();
        publication.publish()
    }

    /// Removes what nothing needs: the files of generations older than the current one,
    /// and stored content that no generation left names. A generation that a reader
    /// still holds ([`Reader`]) is left, with everything it names, and so is content that
    /// a publication in progress has named. Names are removed, then the directory that
    /// held them is synced: generation files first, then content.
    ///
    /// A crash at any point leaves the current generation whole, and at worst some of
    /// what was to go still there; a prune can always be run again. What a prune leaves
    /// behind for a reader goes at the first prune after that reader has finished.
    ///
    /// This writer's own [`Store::snapshot`] holds nothing: a process that prunes decides
    /// for itself what it still reads of an older generation.
    pub fn prune(&self) -> Result<Pruned> {
        let files = &self.files;
        let fs = files.fs.as_ref();
        let layout = &files.layout;
        // No publication of this process changes the generation from here on.
        let _held = self.publishing.lock().unwrap_or_else(|e| e.into_inner());
        let current = files.snapshot()?;
        let mut named: BTreeSet<Digest> = current.references().map(|entry| entry.digest).collect();
        let mut pruned = Pruned::default();

        let generations = layout.generations();
        for file in fs
            .list(&generations)
            .map_err(Error::io("list", &generations))?
        {
            let Some(number) = generation_number(&file).filter(|n| *n < current.number()) else {
                continue;
            };
            // Taken alone, nobody shares it, and nobody can from here to its removal: a
            // reader that comes now finds the name gone and reads the pointer again.
            match fs.try_alone(&file).map_err(Error::io("lock", &file))? {
                Some(alone) => {
                    fs.remove_all(&file).map_err(Error::io("remove", &file))?;
                    drop(alone);
                    pruned.generations_removed.push(number);
                }
                None => {
                    let held = files.generation(number)?;
                    named.extend(held.references().map(|entry| entry.digest));
                    pruned.generations_in_use.push(number);
                }
            }
        }
        pruned.generations_removed.sort_unstable();
        pruned.generations_in_use.sort_unstable();
        if !pruned.generations_removed.is_empty() {
            fs.sync_dir(&generations)
                .map_err(Error::io("sync", &generations))?;
        }

        // Content. Held to the end: a publication that names content now waits, and then
        // finds what was removed gone and stages it again.
        let in_progress = self.in_progress.lock().unwrap_or_else(|e| e.into_inner());
        named.extend(in_progress.values().flatten());
        let objects = layout.objects();
        for file in fs.list(&objects).map_err(Error::io("list", &objects))? {
            let is_named = file
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(Digest::from_hex)
                .is_some_and(|digest| named.contains(&digest));
            if !is_named {
                fs.remove_all(&file).map_err(Error::io("remove", &file))?;
                pruned.objects_removed += 1;
            }
        }
        if pruned.objects_removed > 0 {
            fs.sync_dir(&objects).map_err(Error::io("sync", &objects))?;
        }
        Ok(pruned)
    }
}

/// What a prune did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pruned {
    /// The generations whose files were removed, in order.
    pub generations_removed: Vec<u64>,
    /// The older generations left because a reader holds them, in order. Everything they
    /// name was left too.
    pub generations_in_use: Vec<u64>,
    /// How many files of stored content were removed.
    pub objects_removed: u64,
}

/// The number of a generation file, from its name.
fn generation_number(file: &Path) -> Option<u64> {
    let digits = file.file_name()?.to_str()?.strip_suffix(".json")?;
    let number = digits.parse::<u64>().ok()?;
    (number.to_string() == digits).then_some(number)
}

/// Creates a directory unless it is there, made durable in its parent: with `parents`,
/// after the parents it lacks, each made the same way; without, a missing parent is
/// refused and nothing is made.
fn create_durably(fs: &dyn Fs, dir: &Path, parents: bool) -> Result<()> {
    if fs.exists(dir).map_err(Error::io("look for", dir))? {
        return Ok(());
    }
    let parent = parent_of(dir);
    if parents {
        create_durably(fs, parent, true)?;
    }
    match fs.create_dir(dir) {
        Ok(()) => {}
        // Another process made it since the look: there is a directory, which is the aim.
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        // The parent went since the look.
        Err(error) if !parents && error.kind() == io::ErrorKind::NotFound => {
            return Err(Error::NoParent {
                root: dir.to_path_buf(),
                parent: parent.to_path_buf(),
            });
        }
        Err(error) => return Err(Error::io("create", dir)(error)),
    }
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
    store: &'s Store,
    files: &'s Files,
    /// Its number among the publications of this store value.
    number: u64,
    dir: PathBuf,
    /// The payloads this publication wrote to staging.
    staged: BTreeSet<Digest>,
    /// The names this publication sets outside any run.
    names: BTreeMap<String, Entry>,
    /// The names it sets in runs, by run.
    run_names: BTreeMap<String, BTreeMap<String, Entry>>,
    /// The runs it deletes.
    deleted: BTreeSet<String>,
    /// The runs it makes: each must not be in the generation it is published onto.
    created: BTreeSet<String>,
    /// The names outside runs it makes: each must not be in that generation either.
    made: BTreeSet<String>,
    /// The runs it adds to that must be in that generation.
    extended: BTreeSet<String>,
}

impl Publication<'_> {
    /// Gives `name`, which belongs to no run, this content in the generation to come,
    /// replacing what it had, as an output. The bytes are staged now, and synced, unless
    /// the store already holds them. After an error the same call can be made again.
    pub fn put(&mut self, name: &str, bytes: &[u8]) -> Result<Digest> {
        // A name this publication makes keeps the content it was made with.
        if self.made.contains(name) {
            return Err(Error::NameExists {
                name: name.to_string(),
            });
        }
        let entry = self.stage(bytes, Kind::Output)?;
        let digest = entry.digest;
        self.names.insert(name.to_string(), entry);
        Ok(digest)
    }

    /// Gives the name `name` of the run `run` this content in the generation to come, as
    /// a reference of this kind, replacing what that name of that run had. A run exists
    /// from its first name. Staging is as for [`Publication::put`].
    pub fn put_in(&mut self, run: &str, name: &str, kind: Kind, bytes: &[u8]) -> Result<Digest> {
        check_run_id(run)?;
        let entry = self.stage(bytes, kind)?;
        let digest = entry.digest;
        self.run_names
            .entry(run.to_string())
            .or_default()
            .insert(name.to_string(), entry);
        Ok(digest)
    }

    /// Says that this publication makes the run `run`: it is a new run, and the names
    /// this publication gives it are all it holds. Publishing is then refused with
    /// [`Error::RunExists`], and nothing is published, when the generation it would be
    /// published onto already holds a run of that id. That is looked at inside the
    /// publishing step, on the generation that is current then, so two publications
    /// that each make a run of one id cannot both succeed, whatever each saw before.
    ///
    /// Without this, [`Publication::put_in`] adds names to a run whether or not it
    /// exists, which is how names are added to a run that is already there.
    pub fn create_run(&mut self, run: &str) -> Result<()> {
        check_run_id(run)?;
        self.created.insert(run.to_string());
        self.run_names.entry(run.to_string()).or_default();
        Ok(())
    }

    /// Makes `name`, which belongs to no run, with this content, as a reference of this
    /// kind: it is a new name, and it keeps this content. Publishing is then refused
    /// with [`Error::NameExists`], and nothing is published, when the generation it
    /// would be published onto already holds the name. That is looked at inside the
    /// publishing step, on the generation that is current then, so two publications
    /// that each make one name cannot both succeed, whatever each saw before. It is
    /// refused here when this publication already gives the name content, either way.
    ///
    /// This is how a record that must outlive every run is published (a request id's,
    /// a reservation's): under a name that is made, and so never used twice by code
    /// that makes its names. [`Publication::put`] does not ask whether a name exists.
    /// Staging is as for [`Publication::put`].
    pub fn create(&mut self, name: &str, kind: Kind, bytes: &[u8]) -> Result<Digest> {
        if self.names.contains_key(name) {
            return Err(Error::NameExists {
                name: name.to_string(),
            });
        }
        let entry = self.stage(bytes, kind)?;
        let digest = entry.digest;
        self.names.insert(name.to_string(), entry);
        self.made.insert(name.to_string());
        Ok(digest)
    }

    /// Says that this publication adds to the run `run`, which must be there: publishing
    /// is refused with [`Error::NoSuchRun`], and nothing is published, when the
    /// generation it would be published onto does not hold the run. Without this,
    /// [`Publication::put_in`] makes the run from the names it adds, which is not
    /// wanted of a run that was deleted while its last names were on their way.
    pub fn extend_run(&mut self, run: &str) -> Result<()> {
        check_run_id(run)?;
        self.extended.insert(run.to_string());
        Ok(())
    }

    /// Stages the bytes unless the store or this publication already holds them.
    fn stage(&mut self, bytes: &[u8], kind: Kind) -> Result<Entry> {
        let fs = self.files.fs.as_ref();
        let digest = Digest::of_bytes(bytes);
        // Before the look for the content: once this returns, a prune keeps the content
        // whether it was found or staged; and a prune that is removing content now is
        // waited for, so that the look sees what it left.
        self.store
            .in_progress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(self.number)
            .or_default()
            .insert(digest);
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
        Ok(Entry {
            digest,
            bytes: bytes.len() as u64,
            kind,
        })
    }

    fn staged_path(&self, digest: &Digest) -> PathBuf {
        self.dir.join(digest.hex())
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
        if !self.names.is_empty() || !self.run_names.is_empty() {
            let objects = layout.objects();
            fs.sync_dir(&objects).map_err(Error::io("sync", &objects))?;
        }

        // The generation, immutable from its rename on. It is built on the generation that
        // is current now, not the one that was when this publication began.
        // A publication waits only when it finds the step taken, and only then is it
        // counted as waiting: one that passes straight through never is.
        let publishing = &self.store.publishing;
        let _publishing = match publishing.try_lock() {
            Ok(held) => held,
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            Err(TryLockError::WouldBlock) => {
                self.store.waiting.fetch_add(1, Ordering::SeqCst);
                let held = publishing.lock().unwrap_or_else(|e| e.into_inner());
                self.store.waiting.fetch_sub(1, Ordering::SeqCst);
                held
            }
        };
        let mut generation = files.snapshot()?;
        let number = generation.number.checked_add(1).ok_or_else(|| {
            Error::corrupt(
                &layout.pointer(),
                format!("no generation number is left after {}", generation.number),
            )
        })?;
        generation.number = number;
        for run in &self.deleted {
            if generation.runs.remove(run).is_none() {
                return Err(Error::NoSuchRun { run: run.clone() });
            }
        }
        // The runs this publication deletes are gone from `generation` by now, so no
        // case is left to exempt: a run that is there is a run that exists.
        if let Some(run) = self
            .created
            .iter()
            .find(|run| generation.runs.contains_key(*run))
        {
            return Err(Error::RunExists { run: run.clone() });
        }
        if let Some(run) = self
            .extended
            .iter()
            .find(|run| !generation.runs.contains_key(*run))
        {
            return Err(Error::NoSuchRun { run: run.clone() });
        }
        if let Some(name) = self
            .made
            .iter()
            .find(|name| generation.entries.contains_key(*name))
        {
            return Err(Error::NameExists { name: name.clone() });
        }
        generation.entries.extend(self.names.clone());
        for (run, names) in &self.run_names {
            let run = generation.runs.entry(run.clone()).or_default();
            run.entries.extend(names.clone());
        }
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
        // Visible: the runs this publication made are runs this writer made, whether or
        // not the last sync confirms them.
        self.store
            .made_runs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .extend(self.created.iter().cloned());
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
        // Published, its content is named by a generation; discarded, by nothing.
        self.store
            .in_progress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.number);
    }
}
