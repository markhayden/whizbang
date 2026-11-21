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
pub mod http;
pub mod routes;
pub mod threads;
pub mod token;

use std::collections::{BTreeSet, VecDeque};
use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, RwLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::{Map, Value as Json};
use whizbang_core::contract::{Call, Cancellation, Executor, PreparedCall, RunId, RunState};

pub use http::Deadlines;
pub use token::Token;

/// The port the host listens on unless told otherwise: fixed, so that the page's origin,
/// and with it the page's drafts, is the same from one launch to the next.
pub const PORT_DEFAULT: u16 = 7341;
/// How many connection workers serve requests.
pub const WORKERS: usize = 16;
/// How many uploads hold a worker at once; one more is answered 429.
pub const UPLOADS_LIMIT: usize = 2;
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

/// What the binary hands the host beside the executor: how the page's call becomes a
/// call of an action, what `/api/hello` says beside the version and the store, and what
/// the host's end stops. It holds no HTTP type; the host holds the protocol.
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
    /// A picture the page sent, which decoded inside the core's limits, and the name it
    /// came with: what was made of it, or why not.
    fn picture(&self, _bytes: &[u8], _name: &str) -> Result<Json, String> {
        Err("this build takes no picture from the page yet".to_owned())
    }
    /// The host has stopped and let its executor go: stop what the door holds (the
    /// pack's worker).
    fn closing(&self) {}
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
            look: Duration::from_millis(50),
            deadlines: Deadlines::default(),
        }
    }
}

/// How a host ended: how many runs it started had not ended at the end of the grace.
/// Each is left to be ended as interrupted by the next writer of the store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stopped {
    pub left: usize,
}

/// One run the host started and holds.
pub(crate) struct Held {
    pub run: RunId,
    pub action: String,
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
    pub runs: Vec<Held>,
    /// Places taken by starts that are being admitted.
    pub starting: usize,
    /// Runs that ended and gave their place back, not yet waited for.
    finished: Vec<RunId>,
    jobs: VecDeque<Job>,
    /// Runs the canceller has waited for since the quit.
    waited: BTreeSet<String>,
    pub preparations: VecDeque<Kept>,
    pub uploads: usize,
    quit: Quit,
    done: bool,
    left: usize,
}

pub(crate) struct Shared {
    executor: RwLock<Option<Arc<dyn Executor>>>,
    pub play: Arc<dyn Play>,
    pub token: Token,
    pub settings: Settings,
    pub port: u16,
    pub stopping: AtomicBool,
    state: Mutex<State>,
    changed: Condvar,
}

impl Shared {
    /// The executor, while the host holds it.
    pub fn executor(&self) -> Option<Arc<dyn Executor>> {
        self.executor
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn notify(&self) {
        self.changed.notify_all();
    }

    /// Stops the run `run`, which the host holds: trips its cancellation and hands it
    /// to the canceller, once however often it is asked. Never waits. False when the
    /// host holds no such run.
    pub fn stop(&self, run: &RunId) -> bool {
        let mut state = self.state();
        let Some(held) = state.runs.iter_mut().find(|held| &held.run == run) else {
            return false;
        };
        held.signal.cancel();
        held.stopping = true;
        if !held.cancel_sent {
            held.cancel_sent = true;
            state.jobs.push_back(Job::Cancel(run.clone()));
        }
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
pub struct Host {
    shared: Arc<Shared>,
    address: SocketAddr,
    acceptor: Option<JoinHandle<()>>,
}

impl Host {
    /// Listens on `127.0.0.1` at the settings' port and starts serving: the acceptor,
    /// the workers, the keeper and the canceller, each a counted thread. A port in use is
    /// an error of the kind [`io::ErrorKind::AddrInUse`]; nothing is left running when
    /// this fails.
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
            play,
            token,
            port: address.port(),
            settings,
            stopping: AtomicBool::new(false),
            state: Mutex::new(State {
                runs: Vec::new(),
                starting: 0,
                finished: Vec::new(),
                jobs: VecDeque::new(),
                waited: BTreeSet::new(),
                preparations: VecDeque::new(),
                uploads: 0,
                quit: Quit::Running,
                done: false,
                left: 0,
            }),
            changed: Condvar::new(),
        });
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
                {
                    let mut state = shared.state();
                    state.quit = Quit::Hurried;
                }
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

/// The acceptor: accepts, and hands each connection to a worker, never reading.
fn accept(shared: &Shared, listener: &TcpListener, send: &SyncSender<(TcpStream, Instant)>) {
    for stream in listener.incoming() {
        if shared.state().done {
            return;
        }
        let Ok(stream) = stream else { continue };
        match send.try_send((stream, Instant::now())) {
            Ok(()) => {}
            // Every worker is busy and the queue is full: the connection is closed.
            Err(TrySendError::Full(_)) => {}
            Err(TrySendError::Disconnected(_)) => return,
        }
    }
}

/// A connection worker: one request, one answer, then the next connection.
fn work(shared: &Shared, receive: &Mutex<Receiver<(TcpStream, Instant)>>) {
    loop {
        let next = receive.lock().unwrap_or_else(|e| e.into_inner()).recv();
        let Ok((mut stream, since)) = next else {
            return;
        };
        routes::serve(shared, &mut stream, since);
    }
}

/// The keeper: holds each run's stated time, gives a place back once its run is over,
/// and holds the grace of a quit. It never calls the executor's waiting calls.
fn keep(shared: &Shared, address: SocketAddr) {
    let settings = &shared.settings;
    let mut grace_end: Option<Instant> = None;
    let mut to_wait: BTreeSet<String> = BTreeSet::new();
    loop {
        let now = Instant::now();
        let mut state = shared.state();
        // A quit, the first time it is seen: trip every held cancellation, and hand
        // each run to the canceller to be cancelled and waited for.
        if grace_end.is_none() && state.quit != Quit::Running {
            let asked = match state.quit {
                Quit::Asked(at) => at,
                _ => now,
            };
            grace_end = Some(asked + settings.grace);
            let mut jobs = Vec::new();
            for held in &mut state.runs {
                held.signal.cancel();
                held.stopping = true;
                if !held.cancel_sent {
                    held.cancel_sent = true;
                    jobs.push(Job::Cancel(held.run.clone()));
                }
            }
            let mut waits: Vec<RunId> = state.runs.iter().map(|h| h.run.clone()).collect();
            waits.append(&mut state.finished);
            for run in waits {
                to_wait.insert(run.as_str().to_owned());
                jobs.push(Job::Wait(run));
            }
            state.jobs.extend(jobs);
            shared.notify();
        }
        let looked: Vec<(RunId, Instant, bool)> = state
            .runs
            .iter()
            .map(|held| (held.run.clone(), held.until, held.stopping))
            .collect();
        drop(state);

        // Each run's status, without the lock: it answers during a final publication.
        let executor = shared.executor();
        let mut over = Vec::new();
        let mut late = Vec::new();
        for (run, until, stopping) in looked {
            let state = executor
                .as_ref()
                .and_then(|executor| executor.status_after(&run, u64::MAX).ok());
            let going = matches!(
                state.as_ref().map(|watched| watched.state()),
                Some(RunState::NotEnded { here: true })
            );
            if !going {
                over.push(run);
            } else if !stopping && now >= until {
                late.push(run);
            }
        }
        drop(executor);
        let mut state = shared.state();
        for run in &over {
            state.runs.retain(|held| &held.run != run);
            if grace_end.is_none() {
                state.finished.push(run.clone());
            }
        }
        for run in &late {
            if let Some(held) = state.runs.iter_mut().find(|held| &held.run == run) {
                held.signal.cancel();
                held.stopping = true;
                if !held.cancel_sent {
                    held.cancel_sent = true;
                    state.jobs.push_back(Job::Cancel(run.clone()));
                }
            }
        }
        // A run that ended outside a quit is waited for too, so that its thread holds
        // nothing of the store when the host lets the store go.
        if grace_end.is_none() && !state.finished.is_empty() && state.jobs.is_empty() {
            let finished = std::mem::take(&mut state.finished);
            state.jobs.extend(finished.into_iter().map(Job::Wait));
        }
        if !over.is_empty() || !late.is_empty() {
            shared.notify();
        }
        if let Some(end) = grace_end {
            let all_waited = to_wait.iter().all(|run| state.waited.contains(run));
            let hurried = state.quit == Quit::Hurried;
            if all_waited || hurried || Instant::now() >= end {
                state.left = state.runs.len();
                state.done = true;
                drop(state);
                finish(shared, address);
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
fn finish(shared: &Shared, address: SocketAddr) {
    let executor = shared
        .executor
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    drop(executor);
    shared.play.closing();
    shared.notify();
    let _ = TcpStream::connect_timeout(&address, Duration::from_secs(1));
}

/// The canceller: the one thread that calls [`Executor::cancel`] and
/// [`Executor::wait`], each of which can wait.
fn cancel(shared: &Shared) {
    loop {
        let mut state = shared.state();
        let job = loop {
            if state.done {
                return;
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
                shared.state().waited.insert(run.as_str().to_owned());
                shared.notify();
            }
        }
    }
}
