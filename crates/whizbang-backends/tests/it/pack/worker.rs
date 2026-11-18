//! The supervisor's process (task P3a): the worker is started at the first request and not
//! before, says hello, is handed its three streams and a cleared environment, has its
//! error output read without stopping, kept, bounded and redacted, and is stopped as D7
//! says; and the test kit's guard ends it on every path.
//!
//! Every test here starts the pack through the test kit (`whizbang_testkit::worker`): its
//! fakes, with a lifetime of 120 seconds, held by a guard whose drop kills and waits for
//! the worker and fails the test if a child of the test's process still runs the pack.
//! They are the group `pack-workers` (`.config/nextest.toml`), two at a time.
//!
//! Task P3b adds one call through the supervisor: how far a request got when it fails,
//! what a fault made from a worker's failure says and what it does not, a request from a
//! worker, and the text whizbang sends a worker.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use whizbang_backends::models::ModelsDir;
use whizbang_backends::pack::proto::{Ask, Raster};
use whizbang_backends::pack::queue::Supervisor;
use whizbang_backends::pack::worker::{
    CALL_LIMIT, END_LIMIT, HELLO_LIMIT, LINE_BYTES, LINES_KEPT, LINGERING, PASSED,
};
use whizbang_backends::pack::{Ended, Launch, Worker};
use whizbang_core::capability::{FaultKind, Query, Stage};
use whizbang_core::contract::{Clock, Secrets, SystemClock};
use whizbang_core::identity::{BindingId, Digest};
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;
use whizbang_testkit::scratch::Scratch;
use whizbang_testkit::threads;
use whizbang_testkit::worker::{self as kit, Fixture, Guard};

use super::conformance::{load, segment, three_queries};

/// What the fault `error-output` writes last (the pack's `fakes.py`): a marker, a host and
/// a made-up token.
const MARKER: &str = "fake-error-output-marker-7f3a9c";
const HOST: &str = "pack-worker.example.invalid";
const TOKEN: &str = "made-up-token-5b8e1d0c4a2f6e93";

/// The launch of the fixture's worker.
fn launch(fixture: &Fixture) -> Launch {
    Launch::testing(
        fixture.python.clone(),
        fixture.source.clone(),
        fixture.module.clone(),
        fixture.models_dir.clone(),
        fixture.lifetime_s,
        fixture.more.clone(),
    )
}

/// The supervisor of the pack's fakes with this fault, on this clock, held by the test
/// kit's guard, which kills its worker and waits for it.
pub(super) fn supervised(fault: Option<&str>, clock: Arc<dyn Clock>) -> Guard<Worker> {
    made(fault, |fixture| {
        Worker::new(launch(fixture), clock, Secrets::none())
    })
}

/// The queue of task P4 in front of a supervisor of the pack's fakes with this fault, on
/// this clock, its launch changed by `change`, held by the test kit's guard, which kills
/// its worker and waits for it.
pub(super) fn queued<C: Clock + 'static>(
    fault: Option<&str>,
    clock: Arc<C>,
    change: impl FnOnce(&mut Launch),
) -> Guard<Arc<Supervisor>> {
    queued_over(fault, clock, change).0
}

/// The same, and the fixture's models directory, where the supervisor reads the counts
/// of removals (task P5b) and which the worker is told of. It lives as long as the guard.
pub(super) fn queued_over<C: Clock + 'static>(
    fault: Option<&str>,
    clock: Arc<C>,
    change: impl FnOnce(&mut Launch),
) -> (Guard<Arc<Supervisor>>, PathBuf) {
    let mut root = PathBuf::new();
    let guard = kit::fakes(fault).guard(
        |fixture| {
            let mut launch = launch(fixture);
            change(&mut launch);
            root = fixture.models_dir.clone();
            let models = ModelsDir::at(Arc::new(RealFs), &fixture.models_dir);
            let worker = Worker::new(launch, clock, Secrets::none()).with_spawn(kit::spawn);
            Arc::new(Supervisor::new(worker, models))
        },
        |supervisor| {
            supervisor.kill();
        },
    );
    (guard, root)
}

/// The same, with the supervisor `make` makes from the fixture.
fn made(fault: Option<&str>, make: impl FnOnce(&Fixture) -> Worker) -> Guard<Worker> {
    kit::fakes(fault).guard(
        |fixture| make(fixture).with_spawn(kit::spawn),
        |worker| {
            worker.kill();
        },
    )
}

pub(super) fn system() -> Arc<dyn Clock> {
    Arc::new(SystemClock)
}

/// The machine's clock, moved on by the test: while it rushes, each pause moves it a
/// second and waits for nothing, so a wait of the supervisor's comes to its end at once.
/// Given a budget, it rushes only until it has moved that far, and then waits as the
/// machine's clock does.
#[derive(Default)]
struct Rushed {
    moved_ms: AtomicU64,
    rushing: AtomicBool,
    budget_ms: AtomicU64,
}

impl Rushed {
    fn moved(&self) -> Duration {
        Duration::from_millis(self.moved_ms.load(Ordering::SeqCst))
    }

    /// Rushes from now, through this much of the clock and no more.
    fn rush_through(&self, budget: Duration) {
        let budget = self.moved() + budget;
        self.budget_ms
            .store(budget.as_millis() as u64, Ordering::SeqCst);
        self.rushing.store(true, Ordering::SeqCst);
    }
}

impl Clock for Rushed {
    fn now_ms(&self) -> u64 {
        SystemClock.now_ms() + self.moved_ms.load(Ordering::SeqCst)
    }

    fn now(&self) -> Instant {
        Instant::now() + self.moved()
    }

    fn pause(&self, span: Duration) {
        let budget = self.budget_ms.load(Ordering::SeqCst);
        let within = budget == 0 || self.moved_ms.load(Ordering::SeqCst) < budget;
        if self.rushing.load(Ordering::SeqCst) && within {
            self.moved_ms.fetch_add(1_000, Ordering::SeqCst);
            std::thread::yield_now();
        } else {
            SystemClock.pause(span);
        }
    }
}

/// The lines of the error output that start with `start`, without it.
fn lines_after(worker: &Worker, start: &str) -> Vec<String> {
    worker
        .error_output()
        .iter()
        .filter_map(|line| line.strip_prefix(start).map(str::to_owned))
        .collect()
}

/// The worker's environment, as the fault `error-output` writes it.
fn environment_of(worker: &Worker) -> BTreeMap<String, String> {
    lines_after(worker, "environment ")
        .into_iter()
        .filter_map(|line| {
            let (name, value) = line.split_once('=')?;
            Some((name.to_owned(), value.to_owned()))
        })
        .collect()
}

#[test]
fn a_worker_is_started_at_the_first_request_and_not_before() {
    let mut worker = supervised(None, system());
    // Made, and nothing started: no process, no hello.
    assert_eq!(worker.pid(), None);
    assert!(worker.said().is_none());
    assert_eq!(kit::pack_children(), Vec::<u32>::new());

    // The first request starts it, and hears its hello.
    worker.ready().unwrap();
    let pid = worker.pid().unwrap();
    assert_eq!(kit::pack_children(), vec![pid]);
    let said = worker.said().unwrap().clone();
    assert_eq!(said.device, None, "no runtime was asked yet");
    assert!(said.threads >= 1, "{said:?}");

    // The next finds it running, and starts nothing more.
    worker.ready().unwrap();
    assert_eq!(worker.pid(), Some(pid));
    assert_eq!(kit::pack_children(), vec![pid]);
}

#[test]
fn a_worker_that_says_no_hello_within_its_limit_is_killed_and_the_request_ends_as_unavailable() {
    let clock = Arc::new(Rushed::default());
    clock.rushing.store(true, Ordering::SeqCst);
    let mut worker = supervised(Some("no-hello"), clock.clone());
    let fault = worker.ready().unwrap_err();
    assert_eq!(fault.kind(), FaultKind::Unavailable);
    assert_eq!(
        fault.sentence(),
        "the pack's worker said no hello within 60 seconds and was killed"
    );
    // It waited its whole limit, on the clock it was given, and then killed the worker
    // and waited for it.
    assert!(clock.moved() >= HELLO_LIMIT, "{:?}", clock.moved());
    assert_eq!(worker.ended(), Some(Ended::Killed));
    assert_eq!(worker.pid(), None);
    assert_eq!(kit::pack_children(), Vec::<u32>::new());
}

#[test]
fn a_worker_of_another_protocol_version_is_refused_at_the_hello_and_is_not_left_running() {
    let mut worker = supervised(Some("other-version"), system());
    let fault = worker.ready().unwrap_err();
    assert_eq!(fault.kind(), FaultKind::Unavailable);
    assert_eq!(
        fault.sentence(),
        "the pack's worker speaks another version of the protocol than this whizbang"
    );
    assert!(worker.ended().is_some(), "a worker was started");
    assert_eq!(worker.pid(), None);
    assert!(worker.said().is_none());
    assert_eq!(kit::pack_children(), Vec::<u32>::new());
}

/// Every file under a folder, by its device and number: at most `most`, a few levels
/// down.
fn files_under(folder: &Path, most: usize) -> BTreeSet<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    let mut found = BTreeSet::new();
    let mut folders = vec![folder.to_path_buf()];
    for _ in 0..most {
        let Some(next) = folders.pop() else {
            break;
        };
        for entry in std::fs::read_dir(&next).unwrap().take(most) {
            let entry = entry.unwrap();
            let metadata = std::fs::symlink_metadata(entry.path()).unwrap();
            if metadata.is_dir() {
                folders.push(entry.path());
            } else {
                found.insert((metadata.dev(), metadata.ino()));
            }
        }
    }
    assert!(folders.is_empty(), "the store was not walked whole");
    found
}

#[test]
fn the_worker_is_handed_no_descriptor_but_its_three_and_none_of_the_store() {
    // This process holds a store open to write, its lock taken, while the worker is
    // started.
    let scratch = Scratch::new("worker-store");
    let root = scratch.path().join("store");
    let store = Store::open(&root, Arc::new(RealFs)).unwrap();
    let held = files_under(&root, 64);
    assert!(!held.is_empty());

    let mut worker = supervised(Some("error-output"), system());
    worker.ready().unwrap();

    // Let go here, the store opens again to write while the worker lives: nothing of its
    // lock stayed with the worker.
    drop(store);
    let again = Store::open(&root, Arc::new(RealFs));
    assert!(again.is_ok(), "{:?}", again.err());
    assert!(worker.pid().is_some());

    // What the worker holds, from its own look (`fstat` of every descriptor under 1,024)
    // once its program runs: its three streams, each a pipe; and the copy of its output
    // the worker makes for its frames (P2), a pipe too. Nothing of the store. Its error
    // output is read whole once it has ended.
    assert_eq!(worker.stop(), Some(Ended::Exited(0)));
    let mut held_by_worker = BTreeMap::new();
    for line in lines_after(&worker, "descriptor ") {
        let words: Vec<&str> = line.split(' ').collect();
        assert_eq!(words.len(), 4, "{line}");
        let number: u32 = words[0].parse().unwrap();
        let open_on: (u64, u64) = (words[2].parse().unwrap(), words[3].parse().unwrap());
        held_by_worker.insert(number, (words[1].to_owned(), open_on));
    }
    for stream in 0..3 {
        assert_eq!(
            held_by_worker.get(&stream).map(|(kind, _)| kind.as_str()),
            Some("pipe"),
            "{held_by_worker:?}"
        );
    }
    let beyond: Vec<_> = held_by_worker
        .iter()
        .filter(|(number, _)| **number > 2)
        .collect();
    assert!(beyond.len() <= 1, "{held_by_worker:?}");
    for (kind, open_on) in held_by_worker.values() {
        assert_eq!(kind, "pipe", "{held_by_worker:?}");
        assert!(
            !held.contains(open_on),
            "the worker holds a file of the store"
        );
    }
}

/// The parent's environment the tests choose the worker's from: the locale, a home, a
/// path, proxies, keys of every shape, and the Hub's own variables.
fn parent(home: &Path) -> Vec<(OsString, OsString)> {
    let home = home.to_string_lossy().into_owned();
    [
        ("LANG", "en_US.UTF-8".to_owned()),
        ("LC_ALL", "C".to_owned()),
        ("LC_CTYPE", "C".to_owned()),
        ("HOME", home.clone()),
        ("PATH", format!("{home}/bin:/usr/bin")),
        ("TMPDIR", format!("{home}/tmp")),
        ("XDG_CACHE_HOME", format!("{home}/.cache")),
        ("HF_HOME", format!("{home}/.cache/huggingface")),
        ("HF_TOKEN_PATH", format!("{home}/.cache/huggingface/token")),
        ("HF_TOKEN", "made-up-variable-token-1".to_owned()),
        (
            "HUGGING_FACE_HUB_TOKEN",
            "made-up-variable-token-2".to_owned(),
        ),
        ("HF_HUB_OFFLINE", "0".to_owned()),
        (
            "HTTPS_PROXY",
            "http://proxy.example.invalid:3128".to_owned(),
        ),
        ("http_proxy", "http://proxy.example.invalid:3128".to_owned()),
        (
            "ALL_PROXY",
            "socks5://proxy.example.invalid:1080".to_owned(),
        ),
        ("NO_PROXY", "localhost".to_owned()),
        ("OPENAI_API_KEY", "made-up-key-of-a-provider".to_owned()),
        ("SERVICE_TOKEN", "made-up-token-of-a-service".to_owned()),
        ("cloud_secret", "made-up-secret-of-a-cloud".to_owned()),
        ("PROVIDER_CREDENTIAL", "made-up-credential".to_owned()),
        ("PYTHONPATH", format!("{home}/python")),
        ("PYTHONHOME", format!("{home}/python")),
        ("USER", "someone".to_owned()),
    ]
    .into_iter()
    .map(|(name, value)| (OsString::from(name), OsString::from(value)))
    .collect()
}

#[test]
fn the_workers_environment_holds_the_allow_list_and_no_key() {
    let home = Scratch::new("worker-parent-home");
    // The config names two variables as keys: one of a key's shape, and one that the
    // allow-list would pass.
    let keys = vec!["PROVIDER_CREDENTIAL".to_owned(), "LC_CTYPE".to_owned()];
    let parent = parent(home.path());
    let mut worker = made(Some("error-output"), |fixture| {
        Worker::new(launch(fixture), system(), Secrets::none())
            .with_keys(keys.clone())
            .with_parent_environment(parent.clone())
    });
    worker.ready().unwrap();
    // Its error output is read whole once it has ended.
    assert_eq!(worker.stop(), Some(Ended::Exited(0)));
    let environment = environment_of(&worker);

    // The allow-list, but for what the config names as a key; then the supervisor's own.
    let passed: BTreeSet<&str> = PASSED
        .iter()
        .copied()
        .filter(|name| *name != "LC_CTYPE")
        .collect();
    let set = [
        "HOME",
        "TMPDIR",
        "XDG_CACHE_HOME",
        "HF_HOME",
        "HF_TOKEN_PATH",
        "HF_HUB_OFFLINE",
        "HF_HUB_DISABLE_TELEMETRY",
        "HF_HUB_DISABLE_IMPLICIT_TOKEN",
        "DO_NOT_TRACK",
        "PATH",
        "OMP_NUM_THREADS",
        "MKL_NUM_THREADS",
        "OPENBLAS_NUM_THREADS",
        "VECLIB_MAXIMUM_THREADS",
        "TOKENIZERS_PARALLELISM",
        "PYTORCH_MPS_HIGH_WATERMARK_RATIO",
        "PYTORCH_MPS_LOW_WATERMARK_RATIO",
        "PYTHONDONTWRITEBYTECODE",
        "HF_HUB_DISABLE_PROGRESS_BARS",
    ];
    let expected: BTreeSet<&str> = passed.iter().copied().chain(set).collect();
    // macOS gives every process a variable of its own for the encoding of text, which
    // no parent hands on; it holds three numbers.
    let system = "__CF_USER_TEXT_ENCODING";
    if let Some(value) = environment.get(system) {
        let macos = cfg!(target_os = "macos");
        assert!(macos, "{system} on another system");
        assert!(
            value.split(':').all(|part| part.starts_with("0x")),
            "{value}"
        );
    }
    let names: BTreeSet<&str> = environment
        .keys()
        .map(String::as_str)
        .filter(|name| *name != system)
        .collect();
    assert_eq!(names, expected);
    assert_eq!(environment["LANG"], "en_US.UTF-8");
    assert_eq!(environment["LC_ALL"], "C");

    // No key, no proxy, nothing of the parent's home, its path or its Python.
    let home_text = home.path().to_string_lossy().into_owned();
    for (name, value) in &environment {
        assert!(
            !value.contains(&home_text),
            "{name} holds the parent's home"
        );
        assert!(!value.contains("made-up"), "{name} holds a key");
        assert!(!value.contains("proxy"), "{name} holds a proxy");
    }
    assert_eq!(environment["PATH"], "/usr/bin:/bin");
    assert_eq!(environment["OMP_NUM_THREADS"], "8");
    assert_eq!(environment["TOKENIZERS_PARALLELISM"], "false");
    // The watermarks R1 ended with (ADR 0022): the low one is the high one, under 1.0.
    assert_eq!(environment["PYTORCH_MPS_HIGH_WATERMARK_RATIO"], "0.5");
    assert_eq!(environment["PYTORCH_MPS_LOW_WATERMARK_RATIO"], "0.5");
    assert_eq!(environment["HF_HUB_DISABLE_PROGRESS_BARS"], "1");
}

#[test]
fn the_workers_environment_points_the_hubs_libraries_away_from_the_sign_ins_store() {
    // A sign-in's store in the parent's home, with a made-up token in it, and the
    // variables that point the Hub's libraries at it.
    let home = Scratch::new("worker-sign-in");
    let store = home.path().join(".cache/huggingface");
    std::fs::create_dir_all(&store).unwrap();
    std::fs::write(store.join("token"), "made-up-sign-in-token\n").unwrap();
    let parent = parent(home.path());
    let mut worker = made(Some("error-output"), |fixture| {
        Worker::new(launch(fixture), system(), Secrets::none())
            .with_parent_environment(parent.clone())
    });
    worker.ready().unwrap();

    // The worker's folder, while it runs: empty, outside the parent's home.
    let folder = worker.folder().unwrap().to_path_buf();
    assert!(folder.is_absolute(), "{folder:?}");
    assert!(!folder.starts_with(home.path()));
    assert!(folder.is_dir());
    assert_eq!(
        std::fs::read_dir(&folder).unwrap().count(),
        0,
        "the folder is empty"
    );

    // The folder goes with the worker; its error output is read whole once it has ended.
    assert_eq!(worker.stop(), Some(Ended::Exited(0)));
    assert!(!folder.exists());
    let environment = environment_of(&worker);

    // The Hub's data, the token's file, the cache and the home are that folder, where
    // no token's file was.
    for name in ["HF_HOME", "HOME", "XDG_CACHE_HOME", "TMPDIR"] {
        assert_eq!(Path::new(&environment[name]), folder, "{name}");
    }
    assert_eq!(
        Path::new(&environment["HF_TOKEN_PATH"]),
        folder.join("token")
    );

    // No token is handed by a variable, no call is made to the Hub, nothing is sent.
    assert!(!environment.contains_key("HF_TOKEN"));
    assert!(!environment.contains_key("HUGGING_FACE_HUB_TOKEN"));
    for (name, value) in [
        ("HF_HUB_OFFLINE", "1"),
        ("HF_HUB_DISABLE_TELEMETRY", "1"),
        ("HF_HUB_DISABLE_IMPLICIT_TOKEN", "1"),
        ("DO_NOT_TRACK", "1"),
    ] {
        assert_eq!(environment[name], value, "{name}");
    }
    assert!(
        worker
            .error_output()
            .iter()
            .all(|line| !line.contains("made-up-sign-in-token")
                && !line.contains("made-up-variable-token")),
        "no token reached the worker"
    );
}

#[test]
fn a_dropped_supervisor_closes_the_input_and_the_worker_is_gone_when_the_drop_returns() {
    let before = threads::live();

    // Its input closed, as the end of whizbang's process closes it, the worker ends by
    // itself: exit code 0, the end of its input. Its three threads (its output's reader,
    // its error output's, and the writer of its input), counted with the test's, are
    // joined.
    let mut worker = supervised(None, system());
    worker.ready().unwrap();
    let pid = worker.pid().unwrap();
    assert_eq!(threads::live(), before + 3, "three threads for each worker");
    assert_eq!(worker.stop(), Some(Ended::Exited(0)));
    assert_eq!(threads::live(), before);
    assert!(!kit::runs_the_pack(pid));

    // Dropped, the supervisor does the same, and the worker is gone when the drop
    // returns.
    let mut held = kit::fakes(None).guard(
        |fixture| {
            Some(Worker::new(launch(fixture), system(), Secrets::none()).with_spawn(kit::spawn))
        },
        |worker| {
            if let Some(worker) = worker.as_mut() {
                worker.kill();
            }
        },
    );
    let mut worker = held.take().unwrap();
    worker.ready().unwrap();
    let pid = worker.pid().unwrap();
    assert!(kit::runs_the_pack(pid));
    drop(worker);
    assert!(!kit::runs_the_pack(pid));
    assert_eq!(kit::pack_children(), Vec::<u32>::new());
    assert_eq!(threads::live(), before);
}

#[test]
fn a_worker_that_does_not_end_at_the_end_of_its_input_is_killed_at_the_deadline_and_waited_for() {
    let clock = Arc::new(Rushed::default());
    let mut worker = supervised(Some("deaf-to-end-of-input"), clock.clone());
    worker.ready().unwrap();
    let pid = worker.pid().unwrap();

    // Its input is closed and it lives on: at the deadline, on the clock, it is killed
    // and waited for.
    clock.rushing.store(true, Ordering::SeqCst);
    assert_eq!(worker.stop(), Some(Ended::Killed));
    assert!(clock.moved() >= END_LIMIT, "{:?}", clock.moved());
    assert!(!kit::runs_the_pack(pid));
    assert_eq!(kit::pack_children(), Vec::<u32>::new());
}

#[test]
fn the_error_output_kept_is_bounded_and_a_secret_in_it_is_redacted() {
    // The fault writes 300 lines before its hello, every tenth over 5,000 bytes, more
    // than a pipe holds: the hello comes only because they were read.
    let secrets = Secrets::new([TOKEN.to_owned()]).unwrap();
    let mut worker = made(Some("error-output"), |fixture| {
        Worker::new(launch(fixture), system(), secrets)
    });
    worker.ready().unwrap();
    // What was kept is looked at once the worker has ended, and its reader with it.
    assert_eq!(worker.stop(), Some(Ended::Exited(0)));
    let kept = worker.error_output();

    // The last 256 lines, none over 4,096 bytes; the long ones cut there.
    assert_eq!(kept.len(), LINES_KEPT);
    assert!(kept.iter().all(|line| line.len() <= LINE_BYTES));
    assert!(kept.iter().any(|line| line.len() == LINE_BYTES));
    assert!(!kept.iter().any(|line| line == "error output line 0"));
    assert!(kept.iter().any(|line| line == "error output line 299"));

    // The marker and the host are kept as written; the token is not kept at all.
    let last: Vec<&str> = kept
        .iter()
        .rev()
        .take(3)
        .rev()
        .map(String::as_str)
        .collect();
    assert_eq!(
        last,
        [
            format!("marker {MARKER}").as_str(),
            format!("connecting to host {HOST}").as_str(),
            "token [redacted]",
        ]
    );
    assert!(kept.iter().all(|line| !line.contains(TOKEN)));
}

#[test]
fn the_guard_kills_and_waits_for_its_worker_when_its_test_panics() {
    // A worker deaf to the end of its input, which only a kill or its lifetime ends.
    let started: Arc<Mutex<Option<u32>>> = Arc::default();
    let seen = started.clone();
    let gave_up = catch_unwind(AssertUnwindSafe(move || {
        let mut worker = supervised(Some("deaf-to-end-of-input"), system());
        worker.ready().unwrap();
        *seen.lock().unwrap() = worker.pid();
        panic!("the test gives up with its worker running");
    }));
    assert!(gave_up.is_err());
    let pid = started.lock().unwrap().unwrap();
    assert!(
        !kit::runs_the_pack(pid),
        "the guard killed it and waited for it"
    );
    assert_eq!(kit::pack_children(), Vec::<u32>::new());
}

#[test]
fn a_python_that_is_not_there_or_not_absolute_is_refused_and_says_to_run_setup() {
    for python in ["python3", "/nowhere/bin/python3"] {
        let mut worker = made(None, |fixture| {
            let mut launch = launch(fixture);
            launch.python = PathBuf::from(python);
            Worker::new(launch, system(), Secrets::none())
        });
        let fault = worker.ready().unwrap_err();
        assert_eq!(fault.kind(), FaultKind::Unavailable, "{python}");
        assert_eq!(
            fault.sentence(),
            "the pack's Python is not installed; run whizbang setup"
        );
        assert_eq!(worker.ended(), None, "nothing was started");
        assert_eq!(kit::pack_children(), Vec::<u32>::new());
    }
}

#[test]
fn a_worker_that_ends_before_its_hello_is_named_by_its_exit_code() {
    // A command line the worker refuses: exit code 2, before anything starts.
    let mut worker = made(None, |fixture| {
        let mut launch = launch(fixture);
        launch.models_dir = PathBuf::from("relative/models");
        Worker::new(launch, system(), Secrets::none())
    });
    let fault = worker.ready().unwrap_err();
    assert_eq!(fault.kind(), FaultKind::Unavailable);
    assert_eq!(
        fault.sentence(),
        "the pack's worker ended before its hello: it refused a frame or command line, or could not write (exit code 2)"
    );
    assert_eq!(worker.ended(), Some(Ended::Exited(2)));
    assert!(
        worker
            .error_output()
            .iter()
            .any(|line| line.ends_with("--models-dir must name an absolute folder")),
        "{:?}",
        worker.error_output()
    );

    // Its lifetime, a second here, ends a worker that says no hello: exit code 3.
    let mut worker = made(Some("no-hello"), |fixture| {
        let mut launch = launch(fixture);
        launch.lifetime_s = 1;
        Worker::new(launch, system(), Secrets::none())
    });
    let fault = worker.ready().unwrap_err();
    assert_eq!(
        fault.sentence(),
        "the pack's worker ended before its hello: it reached its lifetime (exit code 3)"
    );
    assert_eq!(worker.ended(), Some(Ended::Exited(3)));
    assert_eq!(kit::pack_children(), Vec::<u32>::new());
}

/// A supervisor of the fakes with this fault, held by a guard as something the test can
/// take out and drop itself.
fn droppable(fault: &str, clock: Arc<dyn Clock>) -> Guard<Option<Worker>> {
    kit::fakes(Some(fault)).guard(
        |fixture| Some(Worker::new(launch(fixture), clock, Secrets::none()).with_spawn(kit::spawn)),
        |worker| {
            if let Some(worker) = worker.as_mut() {
                worker.kill();
            }
        },
    )
}

#[test]
fn a_dropped_supervisor_kills_a_worker_deaf_to_the_end_of_its_input_at_the_deadline() {
    let clock = Arc::new(Rushed::default());
    let mut held = droppable("deaf-to-end-of-input", clock.clone());
    let mut worker = held.take().unwrap();
    worker.ready().unwrap();
    let pid = worker.pid().unwrap();

    // Dropped, it closes the input, which this worker does not end at; at the deadline,
    // on the clock, it is killed, and it is gone when the drop returns.
    clock.rushing.store(true, Ordering::SeqCst);
    drop(worker);
    assert!(clock.moved() >= END_LIMIT, "{:?}", clock.moved());
    assert!(!kit::runs_the_pack(pid));
    assert_eq!(kit::pack_children(), Vec::<u32>::new());
}

/// The word the command line of the process the fault `grandchild` starts holds.
const GRANDCHILD: &str = "whizbang-fake-grandchild";

#[test]
fn a_process_the_worker_started_that_holds_its_streams_open_keeps_neither_stop_nor_drop_waiting() {
    // Two workers, each of which starts a process that holds its streams open for two
    // seconds after it ends. One is stopped, the other dropped, each on a clock that
    // rushes through the wait for its readers.
    let stopped_clock = Arc::new(Rushed::default());
    let dropped_clock = Arc::new(Rushed::default());
    let mut stopped = supervised(Some("grandchild"), stopped_clock.clone());
    let mut held = droppable("grandchild", dropped_clock.clone());
    let mut dropped = held.take().unwrap();
    stopped.ready().unwrap();
    dropped.ready().unwrap();
    // Each worker's own process, its child while the worker lives: only these two are
    // looked at, never another process of the machine.
    let started: Vec<u32> = [stopped.pid().unwrap(), dropped.pid().unwrap()]
        .into_iter()
        .map(|worker| {
            let named = kit::children_of_naming(worker, GRANDCHILD);
            assert_eq!(named.len(), 1, "{named:?}");
            named[0]
        })
        .collect();

    stopped_clock.rushing.store(true, Ordering::SeqCst);
    dropped_clock.rushing.store(true, Ordering::SeqCst);
    let asked = Instant::now();
    assert!(stopped.stop().is_some());
    drop(dropped);
    // Both returned while the two processes still held the streams: within the bound,
    // and not after the two seconds those processes live.
    for pid in &started {
        assert!(
            kit::runs_naming(*pid, GRANDCHILD),
            "{pid} still holds the streams"
        );
    }
    assert!(
        asked.elapsed() < Duration::from_secs(2),
        "{:?}",
        asked.elapsed()
    );
    assert_eq!(kit::pack_children(), Vec::<u32>::new());

    // The readers it let go are kept, and no worker is started while they live: the
    // refusal is at once, and says why.
    assert!(stopped.error_output().contains(&LINGERING.to_owned()));
    let fault = stopped.ready().unwrap_err();
    assert_eq!(fault.kind(), FaultKind::Unavailable);
    assert_eq!(
        fault.sentence(),
        "a process the last worker started still holds its streams open; no worker is started until they close"
    );
    assert_eq!(stopped.pid(), None);

    // The two processes end by themselves, and their streams with them: every reader
    // ends, nothing of the test is left, and a worker starts again.
    assert!(kit::readers_ended(Duration::from_secs(20)));
    // The stopped worker's error output, read whole now, names its process as the one
    // looked at; neither of the two runs any more.
    let said = format!("grandchild {}", started[0]);
    assert!(stopped.error_output().contains(&said), "{said}");
    for pid in &started {
        assert!(
            !kit::runs_naming(*pid, GRANDCHILD),
            "{pid} outlived its life"
        );
    }
    stopped_clock.rushing.store(false, Ordering::SeqCst);
    stopped.ready().unwrap();
    assert!(stopped.stop().is_some());
}

/// A clock whose every pause panics, as one that is broken might.
struct Panics;

impl Clock for Panics {
    fn now_ms(&self) -> u64 {
        SystemClock.now_ms()
    }

    fn pause(&self, _span: Duration) {
        panic!("the clock gives up");
    }
}

#[test]
fn a_panic_between_a_workers_start_and_its_end_leaves_no_process_no_child_unwaited_and_no_folder() {
    // A worker that says no hello: the wait for it asks the clock to pause, which panics
    // while the worker runs.
    let mut worker = supervised(Some("no-hello"), Arc::new(Panics));
    let panicked = catch_unwind(AssertUnwindSafe(|| worker.ready()));
    assert!(panicked.is_err());
    // No process of the test's, running or ended and not waited for, and no folder of
    // a worker's.
    assert_eq!(kit::children(), Vec::new());
    assert_eq!(kit::worker_folders(), Vec::<PathBuf>::new());
    assert_eq!(worker.pid(), None);
    assert!(kit::readers_ended(Duration::from_secs(10)));
}

/// The `.rs` files under `crates/`, outside any `target` folder: at most `most`.
fn sources(most: usize) -> Vec<PathBuf> {
    let crates = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
    let mut found = Vec::new();
    let mut folders = vec![crates.to_path_buf()];
    for _ in 0..most {
        let Some(next) = folders.pop() else {
            break;
        };
        for entry in std::fs::read_dir(&next).unwrap().take(most) {
            let path = entry.unwrap().path();
            if path.is_dir() && path.file_name().is_some_and(|name| name != "target") {
                folders.push(path);
            } else if path.extension().is_some_and(|end| end == "rs") {
                found.push(path);
            }
        }
    }
    assert!(folders.is_empty(), "the crates were not walked whole");
    found
}

#[test]
fn no_test_starts_the_pack_but_through_the_test_kits_guard() {
    // Who names an entry point of the pack's Python package as a module to run (its
    // worker or its fakes), and so may start it: the supervisor, and the test kit's
    // fixture. A test that reads the package's files as files names it otherwise. Who
    // makes a supervisor in a test: the supervisor's own tests here, each through the
    // test kit's guard. The needles are put together, so that this file holds neither.
    let package = ["whizbang", "pack"].join("_");
    let entry_points = [format!("{package}.worker"), format!("{package}.fakes")];
    let crates = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
        .canonicalize()
        .unwrap();
    let mut naming = BTreeSet::new();
    let mut making = BTreeSet::new();
    for path in sources(4_096) {
        let text = std::fs::read_to_string(&path).unwrap();
        let place = path
            .canonicalize()
            .unwrap()
            .strip_prefix(&crates)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        if entry_points
            .iter()
            .any(|entry| text.contains(entry.as_str()))
        {
            naming.insert(place.clone());
        }
        // A launch outside a crate's sources is a test's, made by `Launch::testing`:
        // its entry point and what follows its command line are private otherwise.
        let launches = text.contains("Launch::testing(") || text.contains("Launch {");
        if !place.contains("/src/") && (text.contains("Worker::new(") || launches) {
            making.insert(place);
        }
    }
    let naming: Vec<&str> = naming.iter().map(String::as_str).collect();
    assert_eq!(
        naming,
        [
            "whizbang-backends/src/pack/worker.rs",
            "whizbang-testkit/src/worker.rs"
        ]
    );
    let making: Vec<&str> = making.iter().map(String::as_str).collect();
    // And the binary's tests of the kind `pack` (task P6a), whose one worker of the
    // wiring is started from the fakes inside the guard.
    assert_eq!(
        making,
        [
            "whizbang-backends/tests/it/pack/worker.rs",
            "whizbang/tests/it/pack.rs"
        ]
    );
    let this = std::fs::read_to_string(crates.join(making[0])).unwrap();
    assert!(this.contains("kit::fakes(fault).guard("));
    let binarys = std::fs::read_to_string(crates.join(making[1])).unwrap();
    assert_eq!(binarys.matches("Launch::testing(").count(), 1);
    assert_eq!(binarys.matches("Worker::new(").count(), 1);
    assert_eq!(binarys.matches("kit::fakes(None).guard(").count(), 1);
}

// --- one call (task P3b) ------------------------------------------------------------------------

/// Whether a text holds any of what the fault `error-output` writes: the marker, the host
/// and the token, or a part of the host.
fn holds_any_of_the_error_output(text: &str) -> bool {
    [MARKER, HOST, TOKEN, "example.invalid", "marker", "made-up"]
        .iter()
        .any(|word| text.contains(word))
}

#[test]
fn a_fault_made_from_a_workers_failure_holds_none_of_the_marker_the_host_and_the_token_its_error_output_held()
 {
    let secrets = Secrets::new([TOKEN.to_owned()]).unwrap();
    let mut worker = made(Some("error-output"), |fixture| {
        Worker::new(launch(fixture), system(), secrets)
    });

    // The worker ends a request with an error of its own: a segment of a model it has
    // not loaded. Its sentence is handed on as the worker's, and is not the fault's.
    let (_, ask) = segment("fake-ok", three_queries(), 2);
    let answered = worker.call(ask).unwrap_err();
    assert_eq!(
        answered.fault.kind(),
        FaultKind::Unavailable,
        "{answered:?}"
    );
    assert_eq!(answered.fault.stage(), Stage::Answered);
    assert_eq!(
        answered.fault.sentence(),
        "the pack's worker ended the request with the fault unavailable"
    );
    assert_eq!(answered.said.as_deref(), Some("the model is not loaded"));
    assert_eq!(answered.ended, None, "the worker lives on");
    assert!(worker.pid().is_some());

    // The worker ends inside a call. Just before, it writes the marker, the host and the
    // token to its error output once more.
    worker.call(load("fake-exit")).unwrap();
    let (_, ask) = segment("fake-exit", three_queries(), 2);
    let failed = worker.call(ask).unwrap_err();
    assert_eq!(failed.fault.kind(), FaultKind::Unavailable, "{failed:?}");
    assert_eq!(failed.fault.stage(), Stage::Unresolved);
    assert_eq!(
        failed.fault.sentence(),
        "the pack's worker ended inside the call: it ended inside a call (exit code 4)"
    );
    assert_eq!(failed.ended, Some(Ended::Exited(4)));

    // Its error output is handed on whole, redacted: it was taken once the worker had
    // ended and its reader had read to the end, so it holds what the worker wrote last.
    let last: Vec<&str> = failed.error_output[failed.error_output.len() - 4..]
        .iter()
        .map(String::as_str)
        .collect();
    let marker = format!("marker {MARKER}");
    let host = format!("connecting to host {HOST}");
    assert_eq!(
        last,
        ["in the call r3", &marker, &host, "token [redacted]"],
        "{:?}",
        failed.error_output
    );
    assert!(failed.error_output.iter().all(|line| !line.contains(TOKEN)));

    // Neither fault holds anything of it, in its sentence or in what it prints.
    for fault in [&answered.fault, &failed.fault] {
        for text in [fault.to_string(), format!("{fault:?}")] {
            assert!(!holds_any_of_the_error_output(&text), "{text}");
        }
    }
    // Nor does what a failure prints: neither the error output nor the worker's own
    // sentence is in it, so a careless `{:?}` carries no word of the worker's anywhere.
    for failure in [&answered, &failed] {
        let printed = format!("{failure:?}");
        assert!(!holds_any_of_the_error_output(&printed), "{printed}");
        assert!(!printed.contains("not loaded"), "{printed}");
    }
}

/// A `segment` of a picture of 1,000 by 1,000, four megabytes: more than a pipe holds.
fn larger_than_a_pipe() -> Ask {
    let side = 1_000;
    Ask::Segment {
        model: super::conformance::model("fake-ok"),
        installation: super::conformance::installation(),
        image: Raster {
            binding: BindingId(Digest::of_bytes(b"a large picture")),
            width: side,
            height: side,
            samples: vec![128; (4 * side * side) as usize],
        },
        queries: vec![Query::phrase("a thing").unwrap()],
        max_instances: 1,
    }
}

#[test]
fn a_request_the_worker_does_not_read_ends_at_the_calls_limit_as_timed_out_and_leaves_nothing_behind()
 {
    // The worker answers its hello and then reads nothing: a request larger than the pipe
    // is never taken whole. The write waits within the call's limit, on the clock.
    let clock = Arc::new(Rushed::default());
    let mut worker = supervised(Some("deaf-after-hello"), clock.clone());
    worker.ready().unwrap();
    let pid = worker.pid().unwrap();
    clock.rush_through(CALL_LIMIT);
    let asked = Instant::now();
    let failed = worker.call(larger_than_a_pipe()).unwrap_err();
    assert!(
        asked.elapsed() < Duration::from_secs(10),
        "{:?}",
        asked.elapsed()
    );
    assert!(clock.moved() >= CALL_LIMIT, "{:?}", clock.moved());
    assert_eq!(failed.fault.kind(), FaultKind::TimedOut, "{failed:?}");
    assert_eq!(
        failed.fault.sentence(),
        "the pack's worker did not end the request within 120 seconds and was killed"
    );
    // Part of it was taken by the pipe: it may have been read.
    assert_eq!(failed.fault.stage(), Stage::Unresolved);
    assert_eq!(failed.ended, Some(Ended::Killed));
    // No process, and no thread of the supervisor's: its readers and its writer ended.
    assert!(!kit::runs_the_pack(pid));
    assert_eq!(kit::pack_children(), Vec::<u32>::new());
    assert!(kit::readers_ended(Duration::from_secs(10)));
}

#[test]
fn a_word_to_a_worker_whose_input_is_full_waits_no_longer_than_its_limit() {
    // The worker reads nothing after its hello, and right after it writes a frame
    // whizbang refuses. The request fills the pipe; whizbang then has to tell the worker
    // why the conversation is over, behind it. That word waits five seconds on the
    // clock, the worker's end five more, and then it is killed.
    let clock = Arc::new(Rushed::default());
    let mut worker = supervised(Some("garbage-after-hello"), clock.clone());
    worker.ready().unwrap();
    let pid = worker.pid().unwrap();
    clock.rush_through(END_LIMIT * 2);
    let asked = Instant::now();
    let failed = worker.call(larger_than_a_pipe()).unwrap_err();
    assert!(
        asked.elapsed() < Duration::from_secs(10),
        "{:?}",
        asked.elapsed()
    );
    assert_eq!(failed.fault.kind(), FaultKind::Malformed, "{failed:?}");
    assert_eq!(failed.fault.stage(), Stage::Unresolved);
    assert_eq!(failed.ended, Some(Ended::Killed));
    assert!(!kit::runs_the_pack(pid));
    assert_eq!(kit::pack_children(), Vec::<u32>::new());
    assert!(kit::readers_ended(Duration::from_secs(10)));
}

#[test]
fn a_request_that_could_not_be_written_is_not_sent_and_one_written_and_unanswered_is_unresolved() {
    let mut worker = supervised(None, system());

    // 64 phrases of 512 double quotes, each written as two bytes: the request's header is
    // over 64 KiB, which the core's own bounds allow. It is refused where it would be
    // written, before a worker is started.
    let quotes = "\"".repeat(512);
    let phrases: Vec<Query> = (0..64).map(|_| Query::phrase(&quotes).unwrap()).collect();
    let (_, ask) = segment("fake-ok", phrases.clone(), 1);
    let refused = worker.call(ask).unwrap_err();
    assert_eq!(refused.fault.kind(), FaultKind::TooLarge, "{refused:?}");
    assert_eq!(refused.fault.stage(), Stage::NotSent);
    assert_eq!(refused.ended, None);
    assert_eq!(worker.pid(), None, "no worker was started for it");
    assert_eq!(kit::pack_children(), Vec::<u32>::new());

    // With a worker running, the same: nothing of it reaches the worker, which answers
    // the next request as if it had never been asked.
    worker.call(load("fake-ok")).unwrap();
    let pid = worker.pid();
    let (_, ask) = segment("fake-ok", phrases, 1);
    let refused = worker.call(ask).unwrap_err();
    assert_eq!(refused.fault.kind(), FaultKind::TooLarge);
    assert_eq!(refused.fault.stage(), Stage::NotSent);
    let (_, ask) = segment("fake-ok", three_queries(), 2);
    assert!(worker.call(ask).is_ok());
    assert_eq!(worker.pid(), pid, "the same worker answered");

    // Written, and never answered: the worker ends inside the call. The request left
    // whizbang, so how far it got is not known.
    worker.call(load("fake-exit")).unwrap();
    let (_, ask) = segment("fake-exit", three_queries(), 2);
    let failed = worker.call(ask).unwrap_err();
    assert_eq!(failed.fault.kind(), FaultKind::Unavailable, "{failed:?}");
    assert_eq!(failed.fault.stage(), Stage::Unresolved);
    assert_eq!(failed.ended, Some(Ended::Exited(4)));
    assert_eq!(worker.pid(), None);
}

#[test]
fn a_request_from_the_worker_is_refused_before_its_payload_is_read_and_answered_as_unsupported() {
    // The fault `asks` sends a request of its own at the start of each call: its header
    // and its payload's length, and never the payload. A reader that went on to read the
    // payload would wait for the call's limit, two minutes, past this test's own.
    let mut worker = supervised(Some("asks"), system());
    let asked = Instant::now();
    let failed = worker.call(load("fake-ok")).unwrap_err();
    assert!(
        asked.elapsed() < Duration::from_secs(20),
        "{:?}",
        asked.elapsed()
    );
    assert_eq!(failed.fault.kind(), FaultKind::Malformed, "{failed:?}");
    assert_eq!(failed.fault.stage(), Stage::Unresolved);
    assert_eq!(
        failed.fault.sentence(),
        "the pack's worker sent a request of its own, which this version does not take; it was ended"
    );
    // The worker was answered `unsupported`, and then told the conversation was over,
    // at which it ended by itself.
    assert_eq!(failed.ended, Some(Ended::Exited(0)));
    assert!(
        failed
            .error_output
            .contains(&"asked w1 and was answered unsupported".to_owned()),
        "{:?}",
        failed.error_output
    );
    assert_eq!(worker.pid(), None);
}

#[test]
fn a_text_whizbang_sends_is_cleaned_as_a_text_of_the_protocol_and_the_worker_takes_it() {
    // Whizbang refuses the hello of another version, and tells the worker why with an
    // error whose sentence is the refusal's: "this side speaks pack/1 and the other said
    // pack/2". A `/` is no character of a text of the protocol, and the core's cleaning
    // would put a `?`, which is not one either; the worker's reader would refuse both.
    let mut worker = supervised(Some("other-version"), system());
    let fault = worker.ready().unwrap_err();
    assert_eq!(fault.kind(), FaultKind::Unavailable);
    // The worker took the error as a frame of pack/1, said so, and ended at it.
    assert_eq!(worker.ended(), Some(Ended::Exited(0)));
    assert_eq!(
        lines_after(&worker, "took an error "),
        ["unavailable: this side speaks pack 1 and the other said pack 2"]
    );
}
