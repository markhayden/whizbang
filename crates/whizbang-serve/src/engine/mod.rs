//! The engine API (`SPEC.md` sections 1, 3 and 10; plan m6, task H1; ADR 0045): the MCP
//! server of ADR 0031 spoken over HTTP, with plain routes beside it for pictures in and out
//! and for a run by its id, for a hosted app that uses whizbang as its engine. Written by
//! hand on the standard library, as the stdio server and the playground are.
//!
//! - [`guard`]: who may call, the token, and the reading of a request before it is served.
//! - [`mcp`]: `POST /mcp`, answered as JSON or as an event stream, and a run followed.
//! - [`pictures`]: uploads and downloads.
//! - [`runs`]: a run's state, and its cancel, by its id.
//! - [`threads`]: the engine's counted spawner.
//! - [`Host`]: the listener, its readers and handlers, the runs it started, and stopping.
//!
//! **Reading is apart from serving.** The acceptor hands each connection to one of
//! [`READERS`] readers, which hold it to the guard and read its body (but an upload's),
//! and only then hand it to one of the handlers of its share: [`STREAMS`] for runs being
//! followed, [`UPLOADS`] for uploads, [`RUN_ROUTES`] for a run's state and its cancel, and
//! [`SHORT`] for every other short answer. A stalled stream or upload never takes the run
//! routes' place.
//!
//! **Runs belong to the host.** A run is started by [`Executor::start`] with a
//! [`Cancellation`] and a deadline the host holds, and kept in its table, which no
//! connection owns; the handler that started it follows it for its whole life, and a
//! repeat of its request id takes over its stream. Only the canceller calls the two calls
//! of the executor that can wait, [`Executor::cancel`] and [`Executor::wait`].

pub mod guard;
pub mod mcp;
pub mod pictures;
pub mod runs;
pub mod threads;

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io;
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, RwLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::Value as Json;
use whizbang_core::contract::{Call, Cancellation, Executor, LookedUp, RequestId, RunId, RunState};

pub use crate::play::Refused;
pub use guard::Secret;

use crate::Door;
use guard::Guarded;

/// The one revision of MCP offered over HTTP (ADR 0045).
pub const PROTOCOL_VERSION: &str = "2025-06-18";
/// The header every request but `initialize` names the revision in.
pub const VERSION_HEADER: &str = "mcp-protocol-version";
/// The header that names a run, sent before an answer's body.
pub const RUN_HEADER: &str = "Whizbang-Run";
/// The key of a result's `_meta` that names its run.
pub const RUN_META: &str = "whizbang/run";
/// How many readers read requests.
pub const READERS: usize = 16;
/// The handlers of each share.
pub const STREAMS: usize = 4;
pub const UPLOADS: usize = 2;
pub const RUN_ROUTES: usize = 4;
pub const SHORT: usize = 2;
/// How many ended runs the host remembers having started, the oldest let go first; an
/// older one is still read from the store by its id.
pub const ENDED_REMEMBERED: usize = 4096;
/// The pixel limit of a picture, unless told otherwise.
pub const PIXEL_LIMIT_DEFAULT: u64 = 25_000_000;
/// The stated time of a run, unless told otherwise: the stdio server's.
pub const WAIT_DEFAULT: Duration = Duration::from_secs(900);
/// The grace of a run given up on, and of a stop: the stdio server's.
pub const GRACE: Duration = Duration::from_secs(10);
/// How many requests read and not yet taken by a handler of one share are kept; past it
/// a request is answered 429.
const QUEUED_LIMIT: usize = 32;
/// How many connections accepted and not yet read are kept; past it one is closed.
const ACCEPTED_LIMIT: usize = 64;

/// How long each part of a connection may take (ADR 0045, "Who may call").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Times {
    /// From the connection's acceptance to the end of its headers.
    pub head: Duration,
    /// The fewest bytes a second a body must arrive at, after the head's time.
    pub rate: u64,
    /// How long the client may take to take a plain answer.
    pub answer: Duration,
    /// How long the client may take to take each piece of a stream.
    pub piece: Duration,
}

impl Default for Times {
    fn default() -> Times {
        Times {
            head: Duration::from_secs(2),
            rate: 1024 * 1024,
            answer: Duration::from_secs(10),
            piece: Duration::from_secs(30),
        }
    }
}

/// How long an upload no run took is kept, unless told otherwise (`--uploads-kept-h`).
pub const UPLOADS_KEPT_DEFAULT: Duration = Duration::from_secs(24 * 3600);
/// How often the host lets go of the uploads no run took (plan m6, task D1, item 5).
pub const COLLECT_EVERY: Duration = Duration::from_secs(3600);

/// The time the host's collection reads, in milliseconds since the Unix epoch: the
/// system's, or a test's.
#[derive(Clone)]
pub struct Clock(Arc<dyn Fn() -> u64 + Send + Sync>);

impl Clock {
    /// The system's clock.
    pub fn system() -> Clock {
        Clock(Arc::new(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| {
                    u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
                })
        }))
    }

    /// A clock that reads this.
    pub fn reading(read: Arc<dyn Fn() -> u64 + Send + Sync>) -> Clock {
        Clock(read)
    }

    /// The time now, in milliseconds since the Unix epoch.
    pub fn now_ms(&self) -> u64 {
        (self.0)()
    }
}

impl std::fmt::Debug for Clock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Clock")
    }
}

impl PartialEq for Clock {
    fn eq(&self, other: &Clock) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Clock {}

/// The hourly collection of uploads that no run took (plan m6, task D1, item 5): at the
/// start, then every [`Collection::every`] by [`Collection::clock`], each import no run
/// took that is older than [`Collection::kept`] is let go, through the store's collection
/// ([`Hosted::collect_unused`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Collection {
    pub kept: Duration,
    pub every: Duration,
    pub clock: Clock,
}

impl Default for Collection {
    fn default() -> Collection {
        Collection {
            kept: UPLOADS_KEPT_DEFAULT,
            every: COLLECT_EVERY,
            clock: Clock::system(),
        }
    }
}

/// What the host is started with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// The address to listen on; a port of 0 asks the system for a free one.
    pub listen: SocketAddr,
    /// The names it answers to beside loopback's, each as given, at its port or without one.
    pub host_names: Vec<String>,
    /// The origins a request may name; none by default.
    pub allow_origins: Vec<String>,
    /// The most runs started here that are going at once: 1 to [`STREAMS`], since each has
    /// a stream handler of its own.
    pub calls: usize,
    /// The most pixels of a picture or a mask that arrives.
    pub pixel_limit: u64,
    /// The stated time of each run.
    pub wait: Duration,
    /// How long a run given up on, and a stop, wait for the runs they cancelled.
    pub grace: Duration,
    /// How often a run is looked at.
    pub look: Duration,
    pub times: Times,
    /// The collection of uploads no run took.
    pub collection: Collection,
}

impl Settings {
    /// The defaults, listening at this address.
    pub fn new(listen: SocketAddr) -> Settings {
        Settings {
            listen,
            host_names: Vec::new(),
            allow_origins: Vec::new(),
            calls: crate::CALLS_LIMIT,
            pixel_limit: PIXEL_LIMIT_DEFAULT,
            wait: WAIT_DEFAULT,
            grace: GRACE,
            look: Duration::from_millis(50),
            times: Times::default(),
            collection: Collection::default(),
        }
    }
}

/// What a request id names in the store, by the executor's durable, fingerprinted lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    /// It was never admitted.
    Missing,
    /// It was admitted for this very request: the run it made.
    Same(RunId),
    /// It was admitted for another request.
    Other(RunId),
}

/// What the binary hands the host beside the executor: the stdio server's door (the
/// tools, how a call of one becomes a call of an action, what is said of a run that
/// ended), and what the store holds for the plain routes. It holds no HTTP type.
pub trait Hosted: Door {
    /// A picture that arrived: decoded within the core's limits, with no more than
    /// `pixel_limit` pixels, and imported; `{"image": <identity>}`, or why not.
    fn upload(&self, bytes: &[u8], pixel_limit: u64) -> Result<Json, Refused>;
    /// A mask that arrived, as a grey PNG, for the image of this identity, whose size it
    /// must be: imported; `{"mask": <identity>}`, or why not.
    fn mask(&self, bytes: &[u8], image: &str, pixel_limit: u64) -> Result<Json, Refused>;
    /// The width and height of the image of this identity, when the store holds one: a
    /// mask that arrives for it is held to that size before anything else is done.
    fn image_size(&self, image: &str) -> Option<(u32, u32)>;
    /// An artifact's bytes: a picture as PNG, a mask as its grey PNG.
    fn artifact(&self, id: &str) -> Result<(&'static str, Vec<u8>), Refused>;
    /// The record of a run that has ended, as `whizbang runs show --json` gives it.
    fn record(&self, _run: &RunId) -> Option<Json> {
        None
    }
    /// Lets go of every upload no run took that is older than `older_than_ms` at
    /// `now_ms`, through the store's collection: how many names were let go.
    fn collect_unused(&self, _older_than_ms: u64, _now_ms: u64) -> Result<u64, String> {
        Ok(0)
    }
}

/// How a host ended: how many runs it started (or was admitting) had not ended at the end
/// of the grace. Each is left to be ended as interrupted by the next writer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stopped {
    pub left: usize,
}

/// A connection that a reader is to take over from a run's handler: the request that
/// followed the run again.
pub(crate) struct Follower {
    pub conn: TcpStream,
    pub id: Json,
    pub token: Option<Json>,
    /// The connection's place among those the host holds, let go of with it.
    pub held: Option<Held>,
}

/// One run the host started, or a start it is admitting.
pub(crate) struct Place {
    pub number: u64,
    pub run: Option<RunId>,
    pub signal: Cancellation,
    /// The host tripped its cancellation: at a cancel, at its stated time or at a stop.
    pub stopping: bool,
    /// Its cancel was handed to the canceller (once a run).
    pub cancel_sent: bool,
    /// A handler follows it.
    pub followed: bool,
    /// A request that follows it again, waiting for its handler.
    pub follower: Option<Follower>,
}

/// What the canceller is asked to do.
enum Job {
    Cancel(RunId),
    Wait(RunId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Quit {
    Running,
    Asked(Instant),
    Hurried,
}

/// A connection the host holds: being read, or served.
struct Open {
    conn: TcpStream,
    accepted: Instant,
    reading: bool,
}

pub(crate) struct State {
    pub places: Vec<Place>,
    next_place: u64,
    /// The runs started here that have ended, newest last.
    ended: VecDeque<RunId>,
    ended_set: BTreeSet<String>,
    /// Every run started here that the canceller has not waited for yet.
    unwaited: BTreeSet<String>,
    /// The runs going here whose handler gave up on them at their stated time.
    given_up: BTreeSet<String>,
    jobs: VecDeque<Job>,
    pub uploads: usize,
    open: BTreeMap<u64, Open>,
    next_open: u64,
    quit: Quit,
    done: bool,
}

impl State {
    fn quitting(&self) -> bool {
        self.quit != Quit::Running
    }

    /// Trips the cancellation of a held run and hands it to the canceller, once.
    fn stop_held(&mut self, at: usize) {
        let place = &mut self.places[at];
        place.signal.cancel();
        place.stopping = true;
        if let Some(run) = &place.run
            && !place.cancel_sent
        {
            place.cancel_sent = true;
            let run = run.clone();
            self.jobs.push_back(Job::Cancel(run));
        }
    }

    fn remember_ended(&mut self, run: &RunId) {
        if self.ended_set.insert(run.as_str().to_owned()) {
            self.ended.push_back(run.clone());
        }
        while self.ended.len() > ENDED_REMEMBERED {
            if let Some(oldest) = self.ended.pop_front() {
                self.ended_set.remove(oldest.as_str());
            }
        }
    }
}

/// The executor as the host lends it to one of its threads for a call.
pub(crate) struct Lent<'a> {
    executor: Arc<dyn Executor>,
    lent: &'a AtomicUsize,
}

impl std::ops::Deref for Lent<'_> {
    type Target = dyn Executor;

    fn deref(&self) -> &(dyn Executor + 'static) {
        self.executor.as_ref()
    }
}

impl Drop for Lent<'_> {
    fn drop(&mut self) {
        self.lent.fetch_sub(1, Ordering::SeqCst);
    }
}

/// The senders of the handlers' queues, one a share.
struct Queues {
    streams: SyncSender<Box<Guarded>>,
    uploads: SyncSender<Box<Guarded>>,
    runs: SyncSender<Box<Guarded>>,
    short: SyncSender<Box<Guarded>>,
}

pub(crate) struct Shared {
    executor: RwLock<Option<Arc<dyn Executor>>>,
    lent: AtomicUsize,
    pub hosted: Arc<dyn Hosted>,
    pub token: Secret,
    pub settings: Settings,
    /// The `Host` values a request may name.
    pub names: Vec<String>,
    pub stopping: AtomicBool,
    /// How many collections of uploads no run took have been made.
    collections: std::sync::atomic::AtomicU64,
    state: Mutex<State>,
    changed: Condvar,
    queues: Mutex<Option<Queues>>,
}

/// A place taken for a start while it is admitted: given back when this is dropped,
/// unless the start made a run, so that a refused start, or a handler that panicked,
/// leaks no place.
pub(crate) struct Admitting {
    shared: Arc<Shared>,
    number: u64,
    kept: bool,
    pub signal: Cancellation,
    pub until: Instant,
}

impl Admitting {
    /// The start made this run: the place is its own from now on, followed by the
    /// handler that made it. When the host began to stop while the run was admitted, it is
    /// cancelled and waited for as the rest.
    pub fn made(mut self, run: &RunId) {
        let mut state = self.shared.state();
        let quitting = state.quitting();
        state.unwaited.insert(run.as_str().to_owned());
        if let Some(at) = state.places.iter().position(|p| p.number == self.number) {
            state.places[at].run = Some(run.clone());
            state.places[at].followed = true;
            if quitting || state.places[at].stopping {
                state.places[at].cancel_sent = false;
                state.stop_held(at);
            }
        }
        if quitting {
            state.jobs.push_back(Job::Wait(run.clone()));
        }
        self.kept = true;
        drop(state);
        self.shared.notify();
    }
}

impl Drop for Admitting {
    fn drop(&mut self) {
        if !self.kept {
            let mut state = self.shared.state();
            state.places.retain(|place| place.number != self.number);
            drop(state);
            self.shared.notify();
        }
    }
}

impl Shared {
    /// The executor, while the host holds it, lent and counted.
    pub fn executor(&self) -> Option<Lent<'_>> {
        let executor = self
            .executor
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()?;
        self.lent.fetch_add(1, Ordering::SeqCst);
        Some(Lent {
            executor,
            lent: &self.lent,
        })
    }

    pub fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn notify(&self) {
        self.changed.notify_all();
    }

    /// Waits for a change, `look` at most.
    pub fn wait_a_look<'a>(&self, state: MutexGuard<'a, State>) -> MutexGuard<'a, State> {
        let wait = self.settings.look.max(Duration::from_millis(1));
        self.changed
            .wait_timeout(state, wait)
            .unwrap_or_else(|e| e.into_inner())
            .0
    }

    pub fn done(&self) -> bool {
        self.state().done
    }

    /// Takes a place for a start, when fewer than [`Settings::calls`] are taken and the
    /// host is not stopping, first giving back the places of runs that have ended.
    pub fn admit(self: &Arc<Self>) -> Option<Admitting> {
        self.forget_ended();
        let mut state = self.state();
        if state.places.len() >= self.settings.calls || state.quitting() {
            return None;
        }
        let number = state.next_place;
        state.next_place += 1;
        let signal = Cancellation::new();
        let until = Instant::now() + self.settings.wait;
        state.places.push(Place {
            number,
            run: None,
            signal: signal.clone(),
            stopping: false,
            cancel_sent: false,
            followed: false,
            follower: None,
        });
        Some(Admitting {
            shared: self.clone(),
            number,
            kept: false,
            signal,
            until,
        })
    }

    /// Gives back the place of every run that has ended and that no handler follows,
    /// asking each run's status without the lock (it answers during a final publication),
    /// and hands each to the canceller to be waited for. A status that fails is taken as a
    /// run still going: a place is never given back while its run may not be over.
    pub fn forget_ended(&self) {
        let looked: Vec<RunId> = (self.state().places.iter())
            .filter(|place| !place.followed)
            .filter_map(|place| place.run.clone())
            .collect();
        if looked.is_empty() {
            return;
        }
        let Some(executor) = self.executor() else {
            return;
        };
        let mut over = Vec::new();
        for run in looked {
            if let Ok(watched) = executor.status_after(&run, u64::MAX)
                && !matches!(watched.state(), RunState::NotEnded { here: true })
            {
                over.push(run);
            }
        }
        drop(executor);
        for run in &over {
            self.ended(run);
        }
    }

    /// How many places hold a start being admitted or a run that has not ended, its
    /// status asked without the lock; a status that fails counts as a run going.
    fn going(&self) -> usize {
        let places: Vec<Option<RunId>> = self
            .state()
            .places
            .iter()
            .map(|place| place.run.clone())
            .collect();
        let executor = self.executor();
        places
            .iter()
            .filter(|run| match (run, &executor) {
                (Some(run), Some(executor)) => !matches!(
                    executor.status_after(run, u64::MAX).map(|w| w.state().clone()),
                    Ok(state) if !matches!(state, RunState::NotEnded { here: true })
                ),
                _ => true,
            })
            .count()
    }

    /// A run started here has ended: its place is given back, it is remembered, and it is
    /// handed to the canceller to be waited for. What followed it again and was not yet
    /// taken over comes back.
    pub fn ended(&self, run: &RunId) -> Option<Follower> {
        let mut state = self.state();
        let at = state
            .places
            .iter()
            .position(|p| p.run.as_ref() == Some(run));
        let follower = at.and_then(|at| state.places.remove(at).follower);
        state.given_up.remove(run.as_str());
        state.remember_ended(run);
        state.jobs.push_back(Job::Wait(run.clone()));
        drop(state);
        self.notify();
        follower
    }

    /// Stops the run `run`, which the host holds: trips its cancellation and hands it to
    /// the canceller, once however often it is asked. Never waits. False when the host
    /// holds no such run going.
    pub fn stop(&self, run: &RunId) -> bool {
        let mut state = self.state();
        let Some(at) = state
            .places
            .iter()
            .position(|p| p.run.as_ref() == Some(run))
        else {
            return false;
        };
        state.stop_held(at);
        drop(state);
        self.notify();
        true
    }

    /// Whether the host started this run and remembers it.
    pub fn started_here(&self, run: &RunId) -> bool {
        let state = self.state();
        state.ended_set.contains(run.as_str())
            || state.places.iter().any(|p| p.run.as_ref() == Some(run))
    }

    /// Hands a request that follows a run again to the handler that follows it: the
    /// handler takes it over at its next look. The connection comes back when no handler
    /// follows the run any more.
    pub fn attach(&self, run: &RunId, follower: Follower) -> Result<(), Follower> {
        let mut state = self.state();
        let Some(place) = (state.places.iter_mut())
            .find(|place| place.run.as_ref() == Some(run) && place.followed)
        else {
            return Err(follower);
        };
        let older = place.follower.replace(follower);
        // Let go of outside the lock: its place among the connections takes the lock.
        drop(state);
        if let Some(older) = older {
            let _ = older.conn.shutdown(Shutdown::Both);
        }
        self.notify();
        Ok(())
    }

    /// The request that follows `run` again, if one is waiting.
    pub fn take_follower(&self, run: &RunId) -> Option<Follower> {
        let mut state = self.state();
        let place = (state.places.iter_mut()).find(|place| place.run.as_ref() == Some(run))?;
        place.follower.take()
    }

    /// The handler that followed `run` lets it go: it gave up on it at its stated time.
    /// A request that followed the run again and was not yet taken over comes back, to be
    /// answered as the caller was.
    pub fn unfollow(&self, run: &RunId) -> Option<Follower> {
        let mut state = self.state();
        state.given_up.insert(run.as_str().to_owned());
        let place = (state.places.iter_mut()).find(|p| p.run.as_ref() == Some(run));
        place.and_then(|place| {
            place.followed = false;
            place.follower.take()
        })
    }

    /// Whether the handler of this run, going here, gave up on it at its stated time.
    pub fn given_up(&self, run: &RunId) -> bool {
        self.state().given_up.contains(run.as_str())
    }

    /// As [`Shared::attach`], waiting while a start is being admitted: a repeat that comes
    /// after its run was recorded and before the handler that started it follows it is
    /// handed to that handler once it does, within the head's time at most.
    pub fn attach_waiting(&self, run: &RunId, follower: Follower) -> Result<(), Follower> {
        let until = Instant::now() + self.settings.times.head;
        let mut follower = follower;
        loop {
            match self.attach(run, follower) {
                Ok(()) => return Ok(()),
                Err(back) => follower = back,
            }
            let state = self.state();
            let admitting = state.places.iter().any(|place| place.run.is_none());
            if !admitting || state.done || Instant::now() >= until {
                return Err(follower);
            }
            drop(self.wait_a_look(state));
        }
    }

    /// Whether the run, held here, is being stopped.
    pub fn stopping(&self, run: &RunId) -> bool {
        let state = self.state();
        (state.places.iter()).any(|place| place.run.as_ref() == Some(run) && place.stopping)
    }

    /// Takes a place among the uploads, when fewer than [`UPLOADS`] are taken.
    pub fn take_upload(&self) -> bool {
        let mut state = self.state();
        if state.uploads >= UPLOADS {
            return false;
        }
        state.uploads += 1;
        drop(state);
        self.notify();
        true
    }

    pub fn give_upload(&self) {
        let mut state = self.state();
        state.uploads = state.uploads.saturating_sub(1);
        drop(state);
        self.notify();
    }

    /// Asks the host to stop: the first ask starts the grace, a later one ends it.
    pub fn quit(&self) {
        let mut state = self.state();
        state.quit = match state.quit {
            Quit::Running => {
                self.stopping.store(true, Ordering::SeqCst);
                Quit::Asked(Instant::now())
            }
            _ => Quit::Hurried,
        };
        drop(state);
        self.notify();
    }

    /// A connection accepted at `accepted`, being read: counted, and when more than
    /// [`READERS`] are being read, the one that has waited longest is shut, which ends
    /// its reader's read.
    fn opened(&self, conn: TcpStream, accepted: Instant) -> u64 {
        let mut state = self.state();
        let number = state.next_open;
        state.next_open += 1;
        state.open.insert(
            number,
            Open {
                conn,
                accepted,
                reading: true,
            },
        );
        let reading = state.open.values().filter(|open| open.reading).count();
        if reading > READERS {
            let oldest = (state.open.iter())
                .filter(|(_, open)| open.reading)
                .min_by_key(|(_, open)| open.accepted)
                .map(|(number, _)| *number);
            if let Some(oldest) = oldest
                && let Some(open) = state.open.remove(&oldest)
            {
                let _ = open.conn.shutdown(Shutdown::Both);
            }
        }
        number
    }

    /// The connection was read: it no longer counts among those being read.
    pub(crate) fn read(&self, number: u64) {
        if let Some(open) = self.state().open.get_mut(&number) {
            open.reading = false;
        }
    }

    fn closed(&self, number: u64) {
        self.state().open.remove(&number);
    }

    /// Hands a request that was read to a handler of its share; a share whose queue is
    /// full gives it back.
    fn hand(&self, guarded: Box<Guarded>) -> Result<(), Box<Guarded>> {
        let queues = self.queues.lock().unwrap_or_else(|e| e.into_inner());
        let Some(queues) = queues.as_ref() else {
            return Err(guarded);
        };
        let queue = match guarded.share() {
            guard::Share::Stream => &queues.streams,
            guard::Share::Upload => &queues.uploads,
            guard::Share::Run => &queues.runs,
            guard::Share::Short => &queues.short,
        };
        queue.try_send(guarded).map_err(|error| match error {
            TrySendError::Full(back) | TrySendError::Disconnected(back) => back,
        })
    }
}

/// A connection the host holds, let go of when the request is done.
pub(crate) struct Held {
    shared: Arc<Shared>,
    number: u64,
}

impl Drop for Held {
    fn drop(&mut self) {
        self.shared.closed(self.number);
    }
}

/// What asks a host to stop, from anywhere: a signal, the end of an input, a test.
#[derive(Clone)]
pub struct Stopper(Arc<Shared>);

impl Stopper {
    /// The first call starts the stop and its grace; a later one ends the grace at once.
    pub fn quit(&self) {
        self.0.quit();
    }
}

/// A running host.
pub struct Host {
    shared: Arc<Shared>,
    address: SocketAddr,
    /// Every thread but the canceller, joined at the end.
    threads: Vec<JoinHandle<()>>,
}

type Taken = Arc<Mutex<Receiver<Box<Guarded>>>>;

impl Host {
    /// Listens at the settings' address and starts serving: the acceptor, the readers,
    /// the handlers and the canceller, each a counted thread ([`threads::THREADS_LIMIT`]
    /// in a process). Nothing is left running when this fails.
    pub fn start(
        executor: Arc<dyn Executor>,
        hosted: Arc<dyn Hosted>,
        settings: Settings,
        token: Secret,
    ) -> io::Result<Host> {
        // Each run started here has a stream handler of its own: never more runs than them.
        if settings.calls == 0 || settings.calls > STREAMS {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("the engine runs 1 to {STREAMS} calls at once"),
            ));
        }
        let listener = TcpListener::bind(settings.listen)?;
        let address = listener.local_addr()?;
        let names = guard::names(&settings, address);
        let (streams, take_streams) = sync_channel(QUEUED_LIMIT);
        let (uploads, take_uploads) = sync_channel(QUEUED_LIMIT);
        let (runs, take_runs) = sync_channel(QUEUED_LIMIT);
        let (short, take_short) = sync_channel(QUEUED_LIMIT);
        let shared = Arc::new(Shared {
            executor: RwLock::new(Some(executor)),
            lent: AtomicUsize::new(0),
            hosted,
            token,
            settings,
            names,
            stopping: AtomicBool::new(false),
            collections: std::sync::atomic::AtomicU64::new(0),
            state: Mutex::new(State {
                places: Vec::new(),
                next_place: 0,
                ended: VecDeque::new(),
                ended_set: BTreeSet::new(),
                unwaited: BTreeSet::new(),
                given_up: BTreeSet::new(),
                jobs: VecDeque::new(),
                uploads: 0,
                open: BTreeMap::new(),
                next_open: 0,
                quit: Quit::Running,
                done: false,
            }),
            changed: Condvar::new(),
            queues: Mutex::new(Some(Queues {
                streams,
                uploads,
                runs,
                short,
            })),
        });
        let (send, accepted) = sync_channel::<(TcpStream, Instant, u64)>(ACCEPTED_LIMIT);
        let accepted = Arc::new(Mutex::new(accepted));
        let mut threads = Vec::new();
        let started = (|| -> io::Result<()> {
            let shares: [(&str, usize, Taken); 4] = [
                ("stream", STREAMS, Arc::new(Mutex::new(take_streams))),
                ("upload", UPLOADS, Arc::new(Mutex::new(take_uploads))),
                ("runs", RUN_ROUTES, Arc::new(Mutex::new(take_runs))),
                ("short", SHORT, Arc::new(Mutex::new(take_short))),
            ];
            for (name, count, taken) in shares {
                for _ in 0..count {
                    let (shared, taken) = (shared.clone(), taken.clone());
                    threads.push(threads::spawn(
                        name,
                        Box::new(move || handle(&shared, &taken)),
                    )?);
                }
            }
            for _ in 0..READERS {
                let (shared, accepted) = (shared.clone(), accepted.clone());
                threads.push(threads::spawn(
                    "reader",
                    Box::new(move || read(&shared, &accepted)),
                )?);
            }
            // The canceller is not joined at the end: it may be waiting for a run that
            // ignores its cancellation, and it ends once that wait returns.
            let cancelling = shared.clone();
            threads::spawn("canceller", Box::new(move || cancel(&cancelling)))?;
            let collecting = shared.clone();
            threads.push(threads::spawn(
                "collector",
                Box::new(move || collect(&collecting)),
            )?);
            let accepting = shared.clone();
            threads.push(threads::spawn(
                "acceptor",
                Box::new(move || accept(&accepting, &listener, &send)),
            )?);
            Ok(())
        })();
        let host = Host {
            shared,
            address,
            threads,
        };
        match started {
            Ok(()) => Ok(host),
            Err(error) => {
                // What was made ends: the readers and handlers when their channels are
                // gone, the canceller when it sees the end.
                host.shared.quit();
                host.shared.quit();
                host.finish(Instant::now());
                Err(error)
            }
        }
    }

    /// The address it listens on.
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// Waits, `within` at most, until exactly `count` uploads hold their places among the
    /// [`UPLOADS`] (their headers read, their bodies not yet): whether they do. Each place
    /// taken or given back wakes it, so a caller that must know an upload is held, as a
    /// test does, waits on that and not on a time.
    pub fn uploads_held(&self, count: usize, within: Duration) -> bool {
        let until = Instant::now() + within;
        let mut state = self.shared.state();
        while state.uploads != count {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            state = self
                .shared
                .changed
                .wait_timeout(state, left)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        true
    }

    /// How many collections of uploads no run took it has made: one at its start, then
    /// one each [`Collection::every`] by its clock.
    pub fn collections(&self) -> u64 {
        self.shared.collections.load(Ordering::SeqCst)
    }

    /// What asks it to stop.
    pub fn stopper(&self) -> Stopper {
        Stopper(self.shared.clone())
    }

    /// Waits until the host was asked to stop and has stopped: every run it started was
    /// cancelled and waited for or the grace ended, its executor was let go, the door was
    /// told, its listener closed, and every thread it made has ended.
    pub fn wait(self) -> Stopped {
        let shared = self.shared.clone();
        let mut state = shared.state();
        while !state.quitting() {
            state = shared
                .changed
                .wait(state)
                .unwrap_or_else(|e| e.into_inner());
        }
        let asked = match state.quit {
            Quit::Asked(at) => at,
            _ => Instant::now(),
        };
        let end = asked + shared.settings.grace;
        // Every held cancellation tripped, starts being admitted among them, and every
        // run the host started and has not waited for handed to the canceller.
        for at in 0..state.places.len() {
            state.stop_held(at);
        }
        let waits: Vec<String> = state.unwaited.iter().cloned().collect();
        for run in waits {
            if let Ok(run) = RunId::parse(&run) {
                state.jobs.push_back(Job::Wait(run));
            }
        }
        drop(state);
        shared.notify();
        let left = loop {
            shared.forget_ended();
            let state = shared.state();
            let admitting = state.places.iter().any(|place| place.run.is_none());
            let idle = shared
                .executor()
                .is_none_or(|executor| executor.on_threads() == 0);
            let all_waited = state.unwaited.is_empty() && !admitting && idle;
            let hurried = state.quit == Quit::Hurried;
            if all_waited || hurried || Instant::now() >= end {
                drop(state);
                break shared.going();
            }
            let wait = shared
                .settings
                .look
                .min(end.saturating_duration_since(Instant::now()));
            drop(
                shared
                    .changed
                    .wait_timeout(state, wait.max(Duration::from_millis(1)))
                    .unwrap_or_else(|e| e.into_inner()),
            );
        };
        let hurried = shared.state().quit == Quit::Hurried;
        self.finish(if hurried { Instant::now() } else { end });
        Stopped { left }
    }

    /// The end: the executor let go once no thread of the host holds it (until `until`
    /// at most), the door told, every connection shut, the acceptor woken by a connection
    /// of its own, and every thread joined.
    fn finish(self, until: Instant) {
        let shared = self.shared;
        shared.state().done = true;
        shared.notify();
        // The handlers' queues are closed: a handler ends when it is done with what it
        // holds, and every connection is shut, so nothing it holds waits long.
        shared
            .queues
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        for open in shared.state().open.values() {
            let _ = open.conn.shutdown(Shutdown::Both);
        }
        let executor = shared
            .executor
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        while shared.lent.load(Ordering::SeqCst) > 0 && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(2));
        }
        drop(executor);
        shared.hosted.closing();
        shared.notify();
        let _ = TcpStream::connect_timeout(&self.address, Duration::from_secs(1));
        for thread in self.threads {
            let _ = thread.join();
        }
    }
}

/// The collector (plan m6, task D1, item 5): at the start and then every
/// [`Collection::every`] by its clock, the uploads no run took that are older than
/// [`Collection::kept`] are let go. It looks at its clock every [`Settings::look`], and
/// ends when the host begins to stop. A collection that fails is tried at the next one.
fn collect(shared: &Arc<Shared>) {
    let collection = shared.settings.collection.clone();
    let kept = u64::try_from(collection.kept.as_millis()).unwrap_or(u64::MAX);
    let every = u64::try_from(collection.every.as_millis()).unwrap_or(u64::MAX);
    let mut next = 0u64;
    loop {
        {
            let state = shared.state();
            if state.quitting() || state.done {
                return;
            }
        }
        let now = collection.clock.now_ms();
        if now >= next {
            let _ = shared.hosted.collect_unused(kept, now);
            next = now.saturating_add(every.max(1));
            shared.collections.fetch_add(1, Ordering::SeqCst);
            shared.notify();
        }
        let state = shared.state();
        if state.quitting() || state.done {
            return;
        }
        drop(shared.wait_a_look(state));
    }
}

/// The acceptor: accepts, counts, and hands each connection to a reader, never reading.
/// An error of the system (such as too many open files) is waited out briefly, not spun
/// on.
fn accept(
    shared: &Arc<Shared>,
    listener: &TcpListener,
    send: &SyncSender<(TcpStream, Instant, u64)>,
) {
    for conn in listener.incoming() {
        if shared.done() {
            return;
        }
        let Ok(conn) = conn else {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        };
        let accepted = Instant::now();
        let Ok(copy) = conn.try_clone() else {
            continue;
        };
        let number = shared.opened(copy, accepted);
        match send.try_send((conn, accepted, number)) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => shared.closed(number),
            Err(TrySendError::Disconnected(_)) => return,
        }
    }
}

/// A reader: reads a connection under the guard and hands it to a handler of its share,
/// or answers it. A reader that panics loses its connection and not itself.
fn read(shared: &Arc<Shared>, accepted: &Mutex<Receiver<(TcpStream, Instant, u64)>>) {
    loop {
        let next = accepted.lock().unwrap_or_else(|e| e.into_inner()).recv();
        let Ok((conn, since, number)) = next else {
            return;
        };
        let held = Held {
            shared: shared.clone(),
            number,
        };
        let _ = catch_unwind(AssertUnwindSafe(|| {
            guard::serve(shared, conn, since, held);
        }));
    }
}

/// A handler: takes what a reader read, of its share, and serves it. A handler that
/// panics loses its request and not itself.
fn handle(shared: &Arc<Shared>, taken: &Mutex<Receiver<Box<Guarded>>>) {
    loop {
        let next = taken.lock().unwrap_or_else(|e| e.into_inner()).recv();
        let Ok(guarded) = next else {
            return;
        };
        let _ = catch_unwind(AssertUnwindSafe(|| (*guarded).serve(shared)));
    }
}

/// The canceller: the one thread that calls [`Executor::cancel`] and [`Executor::wait`],
/// each of which can wait. Cancels go before waits, so a run that ignores its
/// cancellation holds up no other run's cancel.
fn cancel(shared: &Shared) {
    loop {
        let mut state = shared.state();
        let job = loop {
            if state.done {
                return;
            }
            let cancel_at = state
                .jobs
                .iter()
                .position(|job| matches!(job, Job::Cancel(_)));
            if let Some(job) = cancel_at.and_then(|at| state.jobs.remove(at)) {
                break job;
            }
            if let Some(job) = state.jobs.pop_front() {
                break job;
            }
            state = shared
                .changed
                .wait(state)
                .unwrap_or_else(|e| e.into_inner());
        };
        drop(state);
        let Some(executor) = shared.executor() else {
            return;
        };
        match job {
            Job::Cancel(run) => {
                let _ = executor.cancel(&run);
            }
            Job::Wait(run) => {
                let _ = executor.wait(&run);
                drop(executor);
                shared.state().unwaited.remove(run.as_str());
                shared.notify();
            }
        }
    }
}

/// The request id's lookup, by the executor's own durable, fingerprinted lookup
/// (`Executor::look_up`, plan m6, task S1, item 8): what the store's admission under it
/// names. Admits, reserves and starts nothing, so it is asked before a place is taken.
pub(crate) fn look_up(
    executor: &dyn Executor,
    id: &RequestId,
    call: &Call,
) -> Result<Lookup, whizbang_core::contract::CallError> {
    let request = whizbang_core::contract::Request::new(call.clone());
    Ok(match executor.look_up(id, &request)? {
        LookedUp::Missing => Lookup::Missing,
        LookedUp::Repeat(earlier) => Lookup::Same(earlier.run().clone()),
        LookedUp::Conflict { run } => Lookup::Other(run),
    })
}
