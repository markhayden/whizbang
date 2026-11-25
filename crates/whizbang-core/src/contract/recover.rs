//! One way to a writer, with recovery before it (ADR 0015, "One way to a writer, with
//! recovery before it").
//!
//! A process can die at any moment, and what it left in a store is put right by the
//! next process that opens the store to write, before that process is handed a writer.
//! [`Opening`] is that entry point, over [`Store::open`]: it takes the writer lock,
//! and whoever then asks it for the writer ([`Opening::writer`]) is answered only once
//! recovery is over. Recovery is a list of parts ([`RecoveryPart`]), each shown the
//! generation the store was found at and adding to one publication of names made
//! once. This module holds the interface and what runs it; what a part recovers is
//! the part's.
//!
//! An opening is in one of three states, shared by every clone of it: initializing,
//! ready, failed. Whoever asks while it is initializing waits for the answer. Failed,
//! the lock is let go, nobody is handed a writer, and the store can still be read
//! ([`Opening::reader`]). One more answer stands beside ready: at the store's limit
//! on a generation file, where recovery's names cannot be published, the writer is
//! handed out for deleting and pruning alone ([`Opening::remover`]), which make the
//! file smaller.

use std::any::Any;
use std::collections::BTreeSet;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};

use crate::capability::Shown;
use crate::store::fs::Fs;
use crate::store::{self, Entry, Generation, Pruned, Publication, Published, Reader, Store};

use super::record::RECORD_KIND;
use super::{Clock, SystemClock};

/// The most characters that are kept of what a part says when it fails.
pub const RECOVERY_WHY_LIMIT: usize = 256;

/// What takes part in the recovery of a store.
///
/// **When it is asked.** Once for a store value, before anyone is handed its writer,
/// by the process that holds the writer lock: so nothing else is published between
/// what the part is shown and what it adds.
///
/// **What it may do.** Add names to the one publication of the recovery
/// ([`Recovering::make`], [`Recovering::make_in`]), each a name that is made once:
/// a part looks at the generation it is shown and adds only what is not there, so
/// that a recovery run again adds nothing. Or fail ([`RecoveryFailed`]), and then
/// nothing of any part is published and nobody is handed a writer. A part that
/// panics has failed.
pub trait RecoveryPart: Send + Sync {
    /// What the part is called, in a failure: a word of the build. It is shown
    /// escaped.
    fn name(&self) -> &'static str;

    /// Looks at the store as it was found, and adds what recovers it.
    fn recover(&self, step: &mut Recovering<'_, '_>) -> Result<(), RecoveryFailed>;
}

/// Why a part could not recover: a sentence, which holds no path. A failure of the
/// store is taken in its words without paths ([`store::Error::without_paths`]); a
/// sentence a part writes itself names what it found by a name of the store and
/// never by where the store is. It is cut to [`RECOVERY_WHY_LIMIT`] characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryFailed {
    why: String,
}

impl RecoveryFailed {
    /// Failed, for this reason.
    pub fn because(why: &str) -> RecoveryFailed {
        RecoveryFailed {
            why: why.chars().take(RECOVERY_WHY_LIMIT).collect(),
        }
    }

    /// The reason.
    pub fn why(&self) -> &str {
        &self.why
    }
}

impl From<store::Error> for RecoveryFailed {
    fn from(error: store::Error) -> RecoveryFailed {
        RecoveryFailed::because(&error.without_paths())
    }
}

/// The recovery of a store, as one part sees it: the generation the store was found
/// at, and a way to add names to the one publication of the recovery.
pub struct Recovering<'a, 's> {
    publication: &'a mut Publication<'s>,
    generation: &'a Generation,
    store: &'a Store,
    now_ms: u64,
    part: &'static str,
    /// The names every part has made in this recovery, outside runs and in runs.
    made: &'a mut BTreeSet<String>,
    made_in: &'a mut BTreeSet<(String, String)>,
}

impl Recovering<'_, '_> {
    /// The generation the store was found at: the one the recovery is published
    /// onto, since this process holds the writer lock and has handed the writer to
    /// nobody.
    pub fn generation(&self) -> &Generation {
        self.generation
    }

    /// The content of an entry of that generation, of either kind.
    pub fn read(&self, entry: &Entry) -> store::Result<Vec<u8>> {
        self.store.read(entry)
    }

    /// Whether this process made the run and has it open ([`Store::made_here`]): a
    /// run that executes here, or that this process abandoned, is not one a dead
    /// process left, and a part leaves it alone.
    ///
    /// **Through the entry point it is always false, today:** a recovery runs once
    /// for a store value, before anyone is handed its writer, so this process has
    /// made no run in it yet. A part asks all the same, so that a recovery that is
    /// ever run later in a process's life ends nothing the process is working on.
    pub fn open_here(&self, run: &str) -> bool {
        self.store.made_here(run)
    }

    /// The time of the recovery, in milliseconds since the Unix epoch: one reading
    /// of the clock, the same for every part.
    pub fn now_ms(&self) -> u64 {
        self.now_ms
    }

    fn refused(&self, what: &str, name: &str, why: &str) -> RecoveryFailed {
        RecoveryFailed::because(&format!(
            "the part {} was to make {what} {}, {why}",
            Shown(self.part),
            Shown(name)
        ))
    }

    /// Adds a name that belongs to no run to the recovery's publication, with this
    /// content, as a record: readable, and never an output. The name is made
    /// ([`Publication::create`]): it is refused here when the generation holds it or
    /// a part made it in this recovery, since a name is made once.
    pub fn make(&mut self, name: &str, bytes: &[u8]) -> Result<(), RecoveryFailed> {
        if self.generation.get(name).is_some() || self.made.contains(name) {
            return Err(self.refused("the name", name, "which is there: a name is made once"));
        }
        self.publication.create(name, RECORD_KIND, bytes)?;
        self.made.insert(name.to_owned());
        Ok(())
    }

    /// Adds a name to a run the generation holds, with this content, as a record.
    /// It is refused when the generation holds no such run, and when the run holds
    /// the name or a part made it in this recovery. The publication is refused
    /// whole when the run is gone by the time it is published.
    pub fn make_in(&mut self, run: &str, name: &str, bytes: &[u8]) -> Result<(), RecoveryFailed> {
        let Some(held) = self.generation.run(run) else {
            return Err(self.refused("a name in the run", run, "which the store does not hold"));
        };
        let key = (run.to_owned(), name.to_owned());
        if held.get(name).is_some() || self.made_in.contains(&key) {
            return Err(self.refused(
                "a name in a run,",
                name,
                "which is there: a name is made once",
            ));
        }
        self.publication.extend_run(run)?;
        self.publication.put_in(run, name, RECORD_KIND, bytes)?;
        self.made_in.insert(key);
        Ok(())
    }
}

/// What recovers a store: the parts, in the order they are asked, and the clock that
/// tells the time of the recovery.
#[derive(Clone)]
pub struct Recovery {
    parts: Vec<Arc<dyn RecoveryPart>>,
    clock: Arc<dyn Clock>,
    /// For tests: the most bytes a generation file the opened store writes may be.
    generation_max: Option<u64>,
}

impl Recovery {
    /// No part: recovery publishes nothing, and the store opens as it did before
    /// there was one.
    pub fn none() -> Recovery {
        Recovery {
            parts: Vec::new(),
            clock: Arc::new(SystemClock),
            generation_max: None,
        }
    }

    /// The same, with one more part, asked after those there are.
    pub fn with_part(mut self, part: Arc<dyn RecoveryPart>) -> Recovery {
        self.parts.push(part);
        self
    }

    /// The same, telling the time by this clock.
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Recovery {
        self.clock = clock;
        self
    }
}

impl Recovery {
    /// For tests: the same, over a store that writes no generation file of more
    /// than this many bytes, its recovery's among them
    /// ([`Store::limit_generations_to`]).
    #[doc(hidden)]
    pub fn limiting_generations_to(mut self, bytes: u64) -> Recovery {
        self.generation_max = Some(bytes);
        self
    }
}

impl std::fmt::Debug for Recovery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = self.parts.iter().map(|part| part.name()).collect();
        f.debug_tuple("Recovery").field(&names).finish()
    }
}

/// Why nobody is handed the writer of a store that was opened to write. Its words
/// hold no path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoWriter {
    /// Recovery failed before its publication was visible: nothing of it was
    /// published, and the store is as it was found. `why` says what failed.
    NothingPublished { why: String },
    /// Recovery's publication became visible, as this generation, and the sync that
    /// makes it durable failed: the whole generation is there and stays, and is not
    /// known to be durable. The next open to write confirms it.
    NotConfirmed { generation: u64 },
    /// Recovery's publication adds names, and the generation file with them would be
    /// `bytes` long where one is at most `limit`. The writer is handed out for
    /// deleting and pruning alone ([`Opening::remover`]), which make the file
    /// smaller; the next open recovers once there is room.
    AtTheLimit { bytes: u64, limit: u64 },
}

impl std::fmt::Display for NoWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NoWriter::NothingPublished { why } => write!(
                f,
                "the store could not be recovered, so it is open to read only: recovery \
                 failed before anything of it was published ({why}); nothing was changed"
            ),
            NoWriter::NotConfirmed { generation } => write!(
                f,
                "the store is open to read only: its recovery was published, as \
                 generation {generation}, and was not confirmed durable; the next open \
                 to write confirms it"
            ),
            NoWriter::AtTheLimit { bytes, limit } => write!(
                f,
                "the store is open for deleting and pruning alone: its recovery could \
                 not be published, because the generation file would be {bytes} bytes \
                 and one is at most {limit}; delete runs and prune, and the next open \
                 recovers it"
            ),
        }
    }
}

impl std::error::Error for NoWriter {}

/// A store opened to write, before and after its recovery: the one way to its writer.
/// Every clone is the same opening, and shares its state.
#[derive(Clone)]
pub struct Opening {
    shared: Arc<Shared>,
}

struct Shared {
    root: PathBuf,
    fs: Arc<dyn Fs>,
    /// Whether the open that gave this value made the store.
    created: bool,
    state: Mutex<State>,
    answered: Condvar,
    /// How many callers are waiting, now, for the recovery to be over.
    waiting: AtomicUsize,
}

enum State {
    /// The lock is held and nobody has asked for the writer yet: the first to ask
    /// recovers.
    Unasked(Box<(Store, Recovery)>),
    /// Recovery is running, on the thread of whoever asked first.
    Initializing,
    Ready(Arc<Store>),
    /// Recovery could not be published at the limit on a generation file: the store
    /// is held for deleting and pruning alone.
    Limited(Arc<Store>, NoWriter),
    /// Recovery failed. The store value is gone, and the lock with it.
    Failed(NoWriter),
}

impl Opening {
    /// Opens the store at `root` to write, as [`Store::open`] does, and takes the
    /// writer lock. Nothing is recovered yet: the first to ask for the writer
    /// recovers.
    pub fn open(root: &Path, fs: Arc<dyn Fs>, recovery: Recovery) -> store::Result<Opening> {
        let store = Store::open(root, fs.clone())?;
        Ok(Opening::over(root, fs, store, recovery))
    }

    /// As [`Opening::open`], over [`Store::open_in_existing_parent`].
    pub fn open_in_existing_parent(
        root: &Path,
        fs: Arc<dyn Fs>,
        recovery: Recovery,
    ) -> store::Result<Opening> {
        let store = Store::open_in_existing_parent(root, fs.clone())?;
        Ok(Opening::over(root, fs, store, recovery))
    }

    /// As [`Opening::open`], over [`Store::open_existing`].
    pub fn open_existing(
        root: &Path,
        fs: Arc<dyn Fs>,
        recovery: Recovery,
    ) -> store::Result<Opening> {
        let store = Store::open_existing(root, fs.clone())?;
        Ok(Opening::over(root, fs, store, recovery))
    }

    fn over(root: &Path, fs: Arc<dyn Fs>, store: Store, recovery: Recovery) -> Opening {
        if let Some(bytes) = recovery.generation_max {
            store.limit_generations_to(bytes);
        }
        Opening {
            shared: Arc::new(Shared {
                root: root.to_path_buf(),
                fs,
                created: store.created(),
                state: Mutex::new(State::Unasked(Box::new((store, recovery)))),
                answered: Condvar::new(),
                waiting: AtomicUsize::new(0),
            }),
        }
    }

    /// Whether the open that gave this value made the store ([`Store::created`]).
    pub fn created(&self) -> bool {
        self.shared.created
    }

    fn state(&self) -> MutexGuard<'_, State> {
        let state = &self.shared.state;
        state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The state once recovery is over: recovers, when nobody has yet, and waits,
    /// when somebody is.
    fn settled(&self) -> MutexGuard<'_, State> {
        let shared = &self.shared;
        let mut state = self.state();
        loop {
            match &*state {
                State::Unasked(_) => break,
                State::Initializing => {
                    shared.waiting.fetch_add(1, Ordering::SeqCst);
                    let waited = shared.answered.wait(state);
                    state = waited.unwrap_or_else(PoisonError::into_inner);
                    shared.waiting.fetch_sub(1, Ordering::SeqCst);
                }
                _ => return state,
            }
        }
        // The first to ask recovers, on its own thread, with the state's lock let
        // go: whoever asks meanwhile finds it initializing, and waits.
        let State::Unasked(unasked) = std::mem::replace(&mut *state, State::Initializing) else {
            return state;
        };
        drop(state);
        // Whatever ends the recovery without an answer, a panic outside a part among
        // it, leaves the opening failed and those who wait answered.
        let mut answer = Answer {
            shared,
            answered: false,
        };
        let (store, recovery) = *unasked;
        let recovered = recover(store, &recovery);
        let mut state = self.state();
        *state = recovered;
        answer.answered = true;
        shared.answered.notify_all();
        state
    }

    /// The writer: the store, to publish to. It is handed out once recovery is over
    /// and was published whole, or had nothing to publish. Whoever asks while
    /// recovery runs waits for it; everyone is answered alike from then on.
    pub fn writer(&self) -> Result<Arc<Store>, NoWriter> {
        match &*self.settled() {
            State::Ready(store) => Ok(store.clone()),
            State::Limited(_, why) | State::Failed(why) => Err(why.clone()),
            State::Unasked(_) | State::Initializing => Err(unanswered()),
        }
    }

    /// The writer, for deleting and pruning alone: handed out when the store is
    /// ready, and also when its recovery could not be published at the limit on a
    /// generation file, which a deletion and a prune make smaller.
    pub fn remover(&self) -> Result<Remover, NoWriter> {
        match &*self.settled() {
            State::Ready(store) | State::Limited(store, _) => Ok(Remover {
                store: store.clone(),
            }),
            State::Failed(why) => Err(why.clone()),
            State::Unasked(_) | State::Initializing => Err(unanswered()),
        }
    }

    /// The store, to read: it is opened as any reader opens it, whatever state the
    /// opening is in, and takes no part in the writer lock.
    pub fn reader(&self) -> store::Result<Reader> {
        Reader::open(&self.shared.root, self.shared.fs.clone())
    }

    /// For tests: how many callers are waiting, now, for the recovery to be over.
    #[doc(hidden)]
    pub fn waiting(&self) -> usize {
        self.shared.waiting.load(Ordering::SeqCst)
    }
}

impl std::fmt::Debug for Opening {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = match &*self.state() {
            State::Unasked(_) => "not asked",
            State::Initializing => "initializing",
            State::Ready(_) => "ready",
            State::Limited(..) => "for deleting and pruning alone",
            State::Failed(_) => "failed",
        };
        f.debug_tuple("Opening").field(&state).finish()
    }
}

/// What is answered where a recovery ended without an answer, which nothing here
/// leaves it to do.
fn unanswered() -> NoWriter {
    NoWriter::NothingPublished {
        why: "recovery ended without an answer".to_owned(),
    }
}

/// The writer of a store, for deleting and pruning alone.
pub struct Remover {
    store: Arc<Store>,
}

impl Remover {
    /// The current generation.
    pub fn snapshot(&self) -> store::Result<Generation> {
        self.store.snapshot()
    }

    /// Deletes runs ([`Store::delete`]).
    pub fn delete(&self, runs: &[&str]) -> store::Result<Published> {
        self.store.delete(runs)
    }

    /// Deletes runs with every run under them, at any depth, in one publication
    /// ([`super::request::with_descendants`]): a run that another run called holds the
    /// inputs it took, and would keep them past its caller's deletion. Gives back what
    /// was deleted, the given runs first.
    pub fn delete_with_descendants(
        &self,
        runs: &[&str],
    ) -> store::Result<(Published, Vec<String>)> {
        let generation = self.store.snapshot()?;
        let all = super::request::with_descendants(&generation, runs, |e| self.store.read(e));
        let named: Vec<&str> = all.iter().map(String::as_str).collect();
        Ok((self.store.delete(&named)?, all))
    }

    /// Lets go of every import no run took that is older than `older_than_ms` at
    /// `now_ms`, in one publication ([`Store::collect_unused_imports`]).
    pub fn collect_unused_imports(
        &self,
        older_than_ms: u64,
        now_ms: u64,
    ) -> store::Result<store::Collected> {
        self.store.collect_unused_imports(older_than_ms, now_ms)
    }

    /// Lets go of a bundle's or an example's entry whole, in one publication
    /// ([`crate::bundle::delete`]; ADR 0048).
    pub fn let_go_of_entry(&self, entry: &str) -> store::Result<Published> {
        let mut publication = self.store.begin()?;
        publication.let_go_of_entry(entry)?;
        publication.publish()
    }

    /// Removes what nothing needs ([`Store::prune`]).
    pub fn prune(&self) -> store::Result<Pruned> {
        self.store.prune()
    }
}

/// What answers for a recovery that ended without an answer: the opening is failed,
/// and whoever waits is told.
struct Answer<'a> {
    shared: &'a Shared,
    answered: bool,
}

impl Drop for Answer<'_> {
    fn drop(&mut self) {
        if !self.answered {
            let state = &self.shared.state;
            *state.lock().unwrap_or_else(PoisonError::into_inner) = State::Failed(unanswered());
            self.shared.answered.notify_all();
        }
    }
}

/// Recovers a store that nobody was handed yet, and answers with the state the
/// opening is in from then on.
///
/// The generation the store was found at is confirmed first, by a sync: the
/// publication that made it may have been visible and not durable, and nothing is
/// built on it before it is. Then each part is asked, in order, and what they add is
/// published once, whole. With nothing added nothing is published.
fn recover(store: Store, recovery: &Recovery) -> State {
    // Dropping the store lets the writer lock go: every failure does.
    let nothing = |why: String| State::Failed(NoWriter::NothingPublished { why });
    if let Err(error) = store.confirm() {
        return nothing(format!(
            "the generation the store was found at could not be confirmed durable: {}",
            error.without_paths()
        ));
    }
    if recovery.parts.is_empty() {
        return State::Ready(Arc::new(store));
    }
    let generation = match store.snapshot() {
        Ok(generation) => generation,
        Err(error) => return nothing(error.without_paths()),
    };
    let now_ms = recovery.clock.now_ms();
    let published = {
        let mut publication = match store.begin() {
            Ok(publication) => publication,
            Err(error) => return nothing(error.without_paths()),
        };
        let (mut made, mut made_in) = (BTreeSet::new(), BTreeSet::new());
        for part in &recovery.parts {
            let name = part.name();
            let mut step = Recovering {
                publication: &mut publication,
                generation: &generation,
                store: &store,
                now_ms,
                part: name,
                made: &mut made,
                made_in: &mut made_in,
            };
            match catch_unwind(AssertUnwindSafe(|| part.recover(&mut step))) {
                Ok(Ok(())) => {}
                Ok(Err(failed)) => {
                    return nothing(format!("its part {} failed: {}", Shown(name), failed.why));
                }
                // What a panic carried is not said: it is whatever the part held.
                Err(panic) => {
                    let_go_of(panic);
                    return nothing(format!("its part {} panicked", Shown(name)));
                }
            }
        }
        if made.is_empty() && made_in.is_empty() {
            None
        } else {
            Some(publication.publish())
        }
    };
    match published {
        None | Some(Ok(_)) => State::Ready(Arc::new(store)),
        Some(Err(store::Error::Unacknowledged { generation, .. })) => {
            State::Failed(NoWriter::NotConfirmed { generation })
        }
        // The names do not fit. Deleting and pruning make the file smaller, so the
        // store is kept, for those alone.
        Some(Err(store::Error::GenerationTooLarge { bytes, limit })) => {
            let why = NoWriter::AtTheLimit { bytes, limit };
            State::Limited(Arc::new(store), why)
        }
        Some(Err(error)) => nothing(error.without_paths()),
    }
}

/// Drops what a panic carried. A value can panic when it is dropped: that panic is
/// caught, and what it carried is not dropped at all.
fn let_go_of(panic: Box<dyn Any + Send>) {
    if let Err(second) = catch_unwind(AssertUnwindSafe(move || drop(panic))) {
        std::mem::forget(second);
    }
}
