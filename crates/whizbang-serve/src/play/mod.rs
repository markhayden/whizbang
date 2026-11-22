//! The playground host (`SPEC.md` sections 3, 7 and 10; plan m5, task H1; ADR 0041): the
//! part of the `whizbang` process that serves the playground's API and its interface
//! over loopback HTTP, written by hand on the standard library.
//!
//! - [`http`]: a request read within its bounds and deadlines, and an answer.
//! - [`guard`]: the checks every request passes before a route is looked at.
//! - [`routes`]: the table of routes, which takes only guarded handlers.
//! - [`assets`]: the interface's files, embedded.
//! - [`threads`]: the host's counted spawner.
//! - [`Host`]: the listener, its workers, the runs it started, and stopping.
//!
//! **Runs belong to the host.** A run the API starts is started by
//! [`Executor::start`] with a [`Cancellation`] and a deadline the host holds, and kept in
//! the host's table; a page that closed holds nothing a run needs. At most
//! [`Settings::calls`] runs the host started are going at once, and a place is never
//! given back while its run is not over.
//!
//! **Stopping** ([`Stopper::quit`]): the host stops taking requests, starts the grace,
//! trips every held cancellation (which never waits) and hands each run to its one
//! canceller thread, which calls [`Executor::cancel`] and [`Executor::wait`], the two
//! calls of the executor that can wait. The keeper, which holds the grace's time, only
//! asks each run's status. At the end of the grace, or once every run was waited for,
//! the host lets its executor go, tells the [`Play`] it is closing, and [`Host::wait`]
//! returns with how many runs had not ended.

pub mod assets;
pub mod guard;
pub mod history;
pub mod http;
pub mod models;
pub mod records;
pub mod routes;
pub mod threads;
pub mod token;

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, RwLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::{Map, Value as Json};
use whizbang_core::contract::{Call, Cancellation, Executor, PreparedCall, RunId, RunState};
use whizbang_core::identity::Digest;

pub use http::Deadlines;
pub use token::Token;

/// The port the host listens on unless told otherwise: fixed, so that the page's origin,
/// and with it the page's drafts, is the same from one launch to the next.
pub const PORT_DEFAULT: u16 = 7341;
/// How many connection workers serve requests.
pub const WORKERS: usize = 16;
/// How many uploads hold a worker at once; one more is answered 429.
pub const UPLOADS_LIMIT: usize = 2;
/// How many thumbnails, and how many artifacts shown whole, are made at once: one more
/// of either is answered 429, as an upload is, so a page of pictures never holds the
/// workers that polls, Stop and Quit need.
pub const DISPLAYS_LIMIT: usize = 2;
/// The most preparations the host keeps under handles, the oldest dropped first.
pub const PREPARATIONS_LIMIT: usize = 64;
/// How long the host keeps a preparation.
pub const PREPARATION_AGE: Duration = Duration::from_secs(3600);
/// The stated time of a run, unless told otherwise.
pub const RUN_LIMIT_DEFAULT: Duration = Duration::from_secs(3600);
/// The grace of a quit: the stdio server's.
pub const GRACE: Duration = Duration::from_secs(10);
/// How many connections accepted and not yet taken by a worker are kept; past it a
/// connection is closed at once.
const QUEUED_LIMIT: usize = 64;
/// How many request ids of runs it started the host remembers, the oldest forgotten
/// first; an older one is still answered by the executor.
pub const REQUESTS_REMEMBERED: usize = 4096;

/// What the binary hands the host beside the executor: how the page's call becomes a
/// call of an action, what `/api/hello` says beside the version and the store, what the
/// store holds of the playground's own (its pictures and drawn locations), and what the
/// host's end stops. It holds no HTTP type; the host holds the protocol.
pub trait Play: Send + Sync {
    /// A call of the action `action` with these arguments, as the page sends them: the
    /// call, and the page's id for the request when it gave one in the arguments; or
    /// why not, in fixed words that say nothing the page sent.
    fn call(
        &self,
        action: &str,
        arguments: &Map<String, Json>,
    ) -> Result<(Call, Option<String>), String>;
    /// What `/api/hello` says beside the version and the store's folder.
    fn hello(&self) -> Map<String, Json> {
        Map::new()
    }
    /// Each argument of the action `action`, as the page names it, that chooses a backend,
    /// with the backends it may name, each as `/api/hello` lists it (`name`, and its own
    /// readiness, `missing`, and `size`): what a preparation is made once for, each (plan
    /// m5, task W1, item 4).
    fn choosers(&self, _action: &str) -> Vec<(String, Vec<Json>)> {
        Vec::new()
    }
    /// A picture the page sent, and the name it came with: decoded within the core's
    /// limits and kept in the store with its name beside it; what was made of it, or why
    /// not.
    fn picture(&self, _bytes: &[u8], _name: &str) -> Result<Json, Refused> {
        Err(Refused::new(
            501,
            "this build takes no picture from the page yet",
        ))
    }
    /// The pictures the playground kept, newest first: at most `limit`, after the one
    /// `after` names when it names one.
    fn pictures(&self, _after: Option<&str>, _limit: usize) -> Result<Json, Refused> {
        Ok(serde_json::json!({ "pictures": [] }))
    }
    /// An artifact of the store, for display: its type of content and its bytes; a
    /// thumbnail at most `edge` pixels on its long side when one is asked for.
    fn artifact(&self, _id: &str, _edge: Option<u32>) -> Result<(&'static str, Vec<u8>), Refused> {
        Err(Refused::new(404, "no artifact has that identity"))
    }
    /// A shape drawn on a picture, committed as a location bound to it: what was made.
    fn location(&self, _drawn: &Json) -> Result<Json, Refused> {
        Err(Refused::new(501, "this build takes no drawn location yet"))
    }
    /// What the store says of a run beside its state: what it spent of what it reserved,
    /// and, once it has ended, its verdict.
    fn facts(&self, _run: &RunId, _ended: bool) -> Map<String, Json> {
        Map::new()
    }
    /// The journal of a run that has ended, as its store keeps it: each line as JSON
    /// (`seq`, `at_ms`, `message`, `event`), in order, and how many events the run
    /// counted and did not keep. The executor's status of an ended run holds no events,
    /// so a poll that comes after the end reads the rest here.
    fn journal(&self, _run: &RunId) -> Option<(Vec<Json>, u64)> {
        None
    }
    /// The config's name of the backend a call that names none uses for this capability:
    /// its default, which preparation resolves to (ADR 0006).
    fn default_backend(&self, _capability: &str) -> Option<String> {
        None
    }
    /// The run the store's admission under this request id names, when it holds one:
    /// a repeat the host has forgotten (it restarted) is answered with that run, before
    /// a place is taken or the call is made again (ADR 0007).
    fn admitted(&self, _id: &whizbang_core::contract::RequestId) -> Option<RunId> {
        None
    }
    /// A script of the fixture host's (`x-fixture-script`), which the real host refuses:
    /// `Err` with fixed words (plan m5, task U1, item 5).
    fn script(&self, _words: &str) -> Result<(), String> {
        Err("this host takes no script".to_owned())
    }
    /// What answers history, the figures, a run's report and its export (plan m5, task
    /// W3), when the door keeps a store.
    fn shelf(&self) -> Option<&dyn history::Shelf> {
        None
    }
    /// The host has stopped and let its executor go: stop what the door holds (the
    /// pack's worker), and let the store go.
    fn closing(&self) {}
    /// The Models screen's models, backends and allowance (plan m5, task M1), when the
    /// door offers them.
    fn models(&self) -> Option<&dyn models::Models> {
        None
    }
}

/// Why a [`Play`] did not do what the page asked: the status and fixed or escaped words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    pub status: u16,
    pub why: String,
}

impl Refused {
    pub fn new(status: u16, why: &str) -> Refused {
        Refused {
            status,
            why: why.to_owned(),
        }
    }
}

/// What the host is started with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// The port, on `127.0.0.1`: 0 asks the system for a free one.
    pub port: u16,
    /// The store's folder, as `/api/hello` shows it.
    pub store: PathBuf,
    /// The stated time of each run the host starts.
    pub run_limit: Duration,
    /// How long a quit waits for the runs it cancelled.
    pub grace: Duration,
    /// The most runs the host started that are going at once.
    pub calls: usize,
    /// How long the host keeps a preparation under its handle.
    pub preparation_age: Duration,
    /// How often the keeper looks at the runs and the grace.
    pub look: Duration,
    pub deadlines: Deadlines,
}

impl Settings {
    /// The defaults, for the store at this folder.
    pub fn new(store: PathBuf) -> Settings {
        Settings {
            port: PORT_DEFAULT,
            store,
            run_limit: RUN_LIMIT_DEFAULT,
            grace: GRACE,
            calls: crate::CALLS_LIMIT,
            preparation_age: PREPARATION_AGE,
            look: Duration::from_millis(50),
            deadlines: Deadlines::default(),
        }
    }
}

/// How a host ended: how many runs it started (or was admitting) had not ended at the
/// end of the grace. Each is left to be ended as interrupted by the next writer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stopped {
    pub left: usize,
}

/// One run the host started and holds, or a start it is admitting.
pub(crate) struct Held {
    /// The place's own number, which finds it again once its start is answered.
    pub place: u64,
    /// The run, once its start was answered; none while it is being admitted.
    pub run: Option<RunId>,
    pub action: String,
    /// The arguments as the page sent them, which a page that opens the run again shows.
    pub arguments: Json,
    pub signal: Cancellation,
    pub until: Instant,
    pub started_ms: u64,
    /// The host tripped its cancellation: at Stop, at its stated time or at a quit.
    pub stopping: bool,
    /// Its cancel was handed to the canceller (once a run).
    pub cancel_sent: bool,
}

/// A preparation the host keeps under a handle.
pub(crate) struct Kept {
    pub handle: String,
    pub prepared: Arc<PreparedCall>,
    pub at: Instant,
}

/// A request id of a run the host started, with what was asked under it: the action,
/// and the digest of its arguments as the page sent them (their names sorted).
pub(crate) struct Asked {
    pub run: RunId,
    pub action: String,
    pub arguments: Digest,
    /// The arguments as sent, which a page that opens the run again shows.
    pub sent: Json,
}

/// The digest of a call's arguments as the page sent them: their JSON, whose object
/// keys are kept sorted, so the same arguments give the same digest however sent.
pub(crate) fn arguments_digest(arguments: &Map<String, Json>) -> Digest {
    Digest::of_bytes(&serde_json::to_vec(arguments).unwrap_or_default())
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

pub(crate) struct State {
    /// The places: runs going, and starts being admitted.
    pub runs: Vec<Held>,
    next_place: u64,
    /// Every run the host started that the canceller has not waited for yet: a quit
    /// is over once this is empty and no start is being admitted.
    unwaited: BTreeSet<String>,
    jobs: VecDeque<Job>,
    pub preparations: VecDeque<Kept>,
    /// The request ids of runs the host started, newest last.
    pub requests: BTreeMap<String, Asked>,
    requests_order: VecDeque<String>,
    pub uploads: usize,
    /// How many thumbnails, and artifacts shown whole, are being made now.
    pub thumbnails: usize,
    pub displays: usize,
    quit: Quit,
    done: bool,
    left: usize,
}

impl State {
    fn quitting(&self) -> bool {
        self.quit != Quit::Running
    }

    /// Trips the cancellation of a held run and hands it to the canceller, once.
    fn stop_held(&mut self, at: usize) {
        let held = &mut self.runs[at];
        held.signal.cancel();
        held.stopping = true;
        if let Some(run) = &held.run
            && !held.cancel_sent
        {
            held.cancel_sent = true;
            let run = run.clone();
            self.jobs.push_back(Job::Cancel(run));
        }
    }

    pub fn remember(&mut self, id: String, asked: Asked) {
        if self.requests.insert(id.clone(), asked).is_none() {
            self.requests_order.push_back(id);
        }
        while self.requests_order.len() > REQUESTS_REMEMBERED {
            if let Some(oldest) = self.requests_order.pop_front() {
                self.requests.remove(&oldest);
            }
        }
    }
}

/// The executor as the host lends it to one of its own threads for a call.
pub(crate) struct Lent<'a> {
    executor: Arc<dyn Executor>,
    lent: &'a std::sync::atomic::AtomicUsize,
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

pub(crate) struct Shared {
    executor: RwLock<Option<Arc<dyn Executor>>>,
    /// How many times the executor is lent out now ([`Shared::executor`]).
    lent: std::sync::atomic::AtomicUsize,
    pub play: Arc<dyn Play>,
    /// The one install, and the one removal, the host runs at a time (task M1).
    pub(crate) installs: models::Installs,
    pub token: Token,
    pub settings: Settings,
    pub port: u16,
    pub stopping: AtomicBool,
    state: Mutex<State>,
    changed: Condvar,
}

/// A place taken for a start while it is admitted: given back when this is dropped,
/// unless the start made a run, so that a refused start, or a handler that panicked,
/// leaks no place.
pub(crate) struct Admitting<'a> {
    shared: &'a Shared,
    pub place: u64,
    kept: bool,
}

impl Admitting<'_> {
    /// The start made this run: the place is its own from now on. When the host began
    /// to quit while the run was admitted, it is cancelled and waited for as the rest.
    pub fn made(mut self, run: RunId) {
        let mut state = self.shared.state();
        let quitting = state.quitting();
        state.unwaited.insert(run.as_str().to_owned());
        if let Some(at) = state.runs.iter().position(|h| h.place == self.place) {
            state.runs[at].run = Some(run.clone());
            if quitting || state.runs[at].stopping {
                state.runs[at].cancel_sent = false;
                state.stop_held(at);
            }
        }
        if quitting {
            state.jobs.push_back(Job::Wait(run));
        }
        self.kept = true;
        drop(state);
        self.shared.notify();
    }
}

impl Drop for Admitting<'_> {
    fn drop(&mut self) {
        if !self.kept {
            let mut state = self.shared.state();
            state.runs.retain(|held| held.place != self.place);
            drop(state);
            self.shared.notify();
        }
    }
}

impl Shared {
    /// The executor, while the host holds it, lent: the host counts what it has lent,
    /// so that its end can wait for its own handlers and canceller to give it back.
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

    /// Takes a place for a start of `action` with these arguments, when fewer than [`Settings::calls`] are
    /// taken and the host is not quitting; first giving back the places of runs that
    /// have ended. None when every place is taken.
    pub fn admit(
        &self,
        action: &str,
        arguments: &Map<String, Json>,
        signal: &Cancellation,
        until: Instant,
        started_ms: u64,
    ) -> Option<Admitting<'_>> {
        self.forget_ended();
        let mut state = self.state();
        if state.runs.len() >= self.settings.calls || state.quitting() {
            return None;
        }
        let place = state.next_place;
        state.next_place += 1;
        state.runs.push(Held {
            place,
            run: None,
            action: action.to_owned(),
            arguments: Json::Object(arguments.clone()),
            signal: signal.clone(),
            until,
            started_ms,
            stopping: false,
            cancel_sent: false,
        });
        Some(Admitting {
            shared: self,
            place,
            kept: false,
        })
    }

    /// Gives back the place of every held run that has ended, asking each run's status
    /// without the lock (it answers during a final publication), and hands each to the
    /// canceller to be waited for. A status that fails is taken as a run still going:
    /// a place is never given back while its run may not be over. What comes back is
    /// the held runs still going, with their stated times and whether they are stopping.
    pub fn forget_ended(&self) -> Vec<(RunId, Instant, bool)> {
        let looked: Vec<RunId> = self
            .state()
            .runs
            .iter()
            .filter_map(|held| held.run.clone())
            .collect();
        let Some(executor) = self.executor() else {
            return Vec::new();
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
        let mut state = self.state();
        for run in &over {
            state.runs.retain(|held| held.run.as_ref() != Some(run));
            state.jobs.push_back(Job::Wait(run.clone()));
        }
        let going = state
            .runs
            .iter()
            .filter_map(|held| Some((held.run.clone()?, held.until, held.stopping)))
            .collect();
        drop(state);
        if !over.is_empty() {
            self.notify();
        }
        going
    }

    /// Stops the run `run`, which the host holds: trips its cancellation and hands it
    /// to the canceller, once however often it is asked. Never waits. False when the
    /// host holds no such run.
    pub fn stop(&self, run: &RunId) -> bool {
        let mut state = self.state();
        let Some(at) = state.runs.iter().position(|h| h.run.as_ref() == Some(run)) else {
            return false;
        };
        state.stop_held(at);
        drop(state);
        self.notify();
        true
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
}

/// What asks a host to stop, from anywhere: the page's Quit, a signal, the end of an
/// input.
#[derive(Clone)]
pub struct Stopper(Arc<Shared>);

impl Stopper {
    /// The first call starts the quit and its grace; a later one ends the grace at once.
    pub fn quit(&self) {
        self.0.quit();
    }
}

/// A running host.
///
/// Its workers are not joined by [`Host::wait`]: once the acceptor has ended, each
/// ends after the connection it holds, which its deadlines bound (the head's, the
/// body's rate and the answer's: at most about 15 seconds with the defaults, longer
/// only for an upload that keeps to the rate), and none holds the executor by then.
pub struct Host {
    shared: Arc<Shared>,
    address: SocketAddr,
    acceptor: Option<JoinHandle<()>>,
}

impl Host {
    /// Listens on `127.0.0.1` at the settings' port and starts serving: the acceptor,
    /// the workers, the keeper and the canceller, each a counted thread
    /// ([`threads::THREADS_LIMIT`] in a process). A port in use is an error of the kind
    /// [`io::ErrorKind::AddrInUse`]; nothing is left running when this fails.
    pub fn start(
        executor: Arc<dyn Executor>,
        play: Arc<dyn Play>,
        settings: Settings,
        token: Token,
    ) -> io::Result<Host> {
        let listener = TcpListener::bind(("127.0.0.1", settings.port))?;
        let address = listener.local_addr()?;
        let shared = Arc::new(Shared {
            executor: RwLock::new(Some(executor)),
            lent: std::sync::atomic::AtomicUsize::new(0),
            play,
            installs: models::Installs::default(),
            token,
            port: address.port(),
            settings,
            stopping: AtomicBool::new(false),
            state: Mutex::new(State {
                runs: Vec::new(),
                next_place: 0,
                unwaited: BTreeSet::new(),
                jobs: VecDeque::new(),
                preparations: VecDeque::new(),
                requests: BTreeMap::new(),
                requests_order: VecDeque::new(),
                uploads: 0,
                thumbnails: 0,
                displays: 0,
                quit: Quit::Running,
                done: false,
                left: 0,
            }),
            changed: Condvar::new(),
        });
        shared.installs.wakes(&shared);
        let (send, receive) = sync_channel::<(TcpStream, Instant)>(QUEUED_LIMIT);
        let receive = Arc::new(Mutex::new(receive));
        let started = (|| -> io::Result<JoinHandle<()>> {
            for _ in 0..WORKERS {
                let (shared, receive) = (shared.clone(), receive.clone());
                threads::spawn("worker", Box::new(move || work(&shared, &receive)))?;
            }
            let keeping = shared.clone();
            threads::spawn("keeper", Box::new(move || keep(&keeping, address)))?;
            let cancelling = shared.clone();
            threads::spawn("canceller", Box::new(move || cancel(&cancelling)))?;
            let accepting = shared.clone();
            threads::spawn(
                "acceptor",
                Box::new(move || accept(&accepting, &listener, &send)),
            )
        })();
        match started {
            Ok(acceptor) => Ok(Host {
                shared,
                address,
                acceptor: Some(acceptor),
            }),
            Err(error) => {
                // What was made ends: the keeper finishes at once, and the workers end
                // when the acceptor's side of their channel is gone.
                shared.state().quit = Quit::Hurried;
                shared.stopping.store(true, Ordering::SeqCst);
                shared.notify();
                Err(error)
            }
        }
    }

    /// The address it listens on.
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// What asks it to stop.
    pub fn stopper(&self) -> Stopper {
        Stopper(self.shared.clone())
    }

    /// Waits until the host has stopped: its runs were waited for or the grace ended,
    /// its executor was let go, and its listener closed.
    pub fn wait(mut self) -> Stopped {
        let mut state = self.shared.state();
        while !state.done {
            state = self
                .shared
                .changed
                .wait(state)
                .unwrap_or_else(|e| e.into_inner());
        }
        let left = state.left;
        drop(state);
        if let Some(acceptor) = self.acceptor.take() {
            let _ = acceptor.join();
        }
        Stopped { left }
    }
}

/// The acceptor: accepts, and hands each connection to a worker, never reading. An
/// error of the system (such as too many open files) is waited out briefly, not spun on.
fn accept(shared: &Shared, listener: &TcpListener, send: &SyncSender<(TcpStream, Instant)>) {
    for stream in listener.incoming() {
        if shared.state().done {
            return;
        }
        let Ok(stream) = stream else {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        };
        match send.try_send((stream, Instant::now())) {
            Ok(()) => {}
            // Every worker is busy and the queue is full: the connection is closed.
            Err(TrySendError::Full(_)) => {}
            Err(TrySendError::Disconnected(_)) => return,
        }
    }
}

/// A connection worker: one request, one answer, then the next connection. A handler
/// that panics loses its connection and not the worker.
fn work(shared: &Shared, receive: &Mutex<Receiver<(TcpStream, Instant)>>) {
    loop {
        let next = receive.lock().unwrap_or_else(|e| e.into_inner()).recv();
        let Ok((mut stream, since)) = next else {
            return;
        };
        let _ = catch_unwind(AssertUnwindSafe(|| {
            routes::serve(shared, &mut stream, since);
        }));
    }
}

/// The keeper: holds each run's stated time, gives a place back once its run is over,
/// and holds the grace of a quit. It never calls the executor's waiting calls.
fn keep(shared: &Shared, address: SocketAddr) {
    let settings = &shared.settings;
    let mut grace_end: Option<Instant> = None;
    loop {
        let now = Instant::now();
        {
            let mut state = shared.state();
            // A quit, the first time it is seen: trip every held cancellation, starts
            // being admitted among them, and hand every run the host started and has
            // not waited for to the canceller: first each cancel, then each wait.
            if grace_end.is_none() && state.quitting() {
                let asked = match state.quit {
                    Quit::Asked(at) => at,
                    _ => now,
                };
                grace_end = Some(asked + settings.grace);
                shared.installs.close();
                for at in 0..state.runs.len() {
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
            }
        }
        let going = shared.forget_ended();
        let mut state = shared.state();
        if grace_end.is_none() {
            for (run, until, stopping) in going {
                if !stopping
                    && now >= until
                    && let Some(at) = state.runs.iter().position(|h| h.run.as_ref() == Some(&run))
                {
                    state.stop_held(at);
                    shared.notify();
                }
            }
        }
        if let Some(end) = grace_end {
            let admitting = state.runs.iter().any(|held| held.run.is_none());
            // Every run waited for, and every thread the executor started (the runs
            // those runs called among them) has let go of the store: then dropping the
            // executor lets the store's writer go at once.
            let idle = shared
                .executor()
                .is_none_or(|executor| executor.on_threads() == 0);
            let all_waited =
                state.unwaited.is_empty() && !admitting && idle && shared.installs.idle();
            let hurried = state.quit == Quit::Hurried;
            if all_waited || hurried || Instant::now() >= end {
                state.left = state.runs.len();
                state.done = true;
                drop(state);
                // A second quit waits for nobody.
                finish(shared, address, if hurried { Instant::now() } else { end });
                return;
            }
        }
        let wait = match grace_end {
            Some(end) => settings
                .look
                .min(end.saturating_duration_since(Instant::now())),
            None => settings.look,
        };
        let _ = shared
            .changed
            .wait_timeout(state, wait.max(Duration::from_millis(1)))
            .unwrap_or_else(|e| e.into_inner());
    }
}

/// The end: the executor let go, the door told, everyone waiting woken, and the acceptor
/// woken by a connection of its own so that it sees the end and closes the listener.
///
/// A handler or the canceller may still hold the executor lent for a moment (a start
/// answered as the quit came, say): the end waits, until the grace's end at most, for
/// each to give it back, so that dropping the host's own lets the store's writer go.
/// What the caller of [`Host::start`] holds of the executor beside is its own.
fn finish(shared: &Shared, address: SocketAddr, until: Instant) {
    let executor = shared
        .executor
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    while shared.lent.load(Ordering::SeqCst) > 0 && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(2));
    }
    drop(executor);
    shared.play.closing();
    shared.notify();
    let _ = TcpStream::connect_timeout(&address, Duration::from_secs(1));
}

/// The canceller: the one thread that calls [`Executor::cancel`] and
/// [`Executor::wait`], each of which can wait. Cancels go before waits, so a run that
/// ignores its cancellation holds up no other run's cancel.
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
