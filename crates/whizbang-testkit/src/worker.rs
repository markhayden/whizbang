//! The pack's worker in a test: the one way a test starts it (plan 1C, "Rules", "A worker
//! a test started"), with fake models and fake faults, and the guard that kills it and
//! waits for it on every path out of the test, a panic included.
//!
//! - [`fakes`] makes the fixture: the pinned Python, the pack's source, the entry point
//!   `whizbang_pack.fakes` with a fault or none, a models directory of the test's own,
//!   and a lifetime of [`LIFETIME_S`], always.
//! - [`Fixture::guard`] makes the supervisor from it (the test kit does not depend on
//!   the crate that holds the supervisor, so the test makes it) and hands back a
//!   [`Guard`] that holds it. The guard's drop ends the worker by the way the test gave
//!   (a kill, and a wait), and then looks at the processes of the machine: a child of
//!   the test's process that runs the pack is a failure of the test. Only the test's own
//!   children are looked at; nothing is signalled.
//! - [`spawn`] is what the supervisor makes its threads with in a test: through
//!   [`crate::threads`], so that they are counted with the test's others.
//!
//! **How the Python is found:** as `scripts/test-pack.sh` finds it. `uv` is started
//! once in a test process, offline, with no user's configuration, with downloads of a
//! Python turned off and the bounds of the row of "Rules" on `uv`, in a cleared
//! environment (`PATH`, `HOME`, `TMPDIR` and `UV_PYTHON_INSTALL_DIR` of the test's), to
//! find the Python `pack/.python-version` pins among those it manages. It has 60 seconds,
//! then it is killed and waited for. A Python on `PATH` is never taken. Where none is
//! found the test fails and names the one command that installs it.
//!
//! The tests that start a worker are the group `pack-workers` in `.config/nextest.toml`,
//! two at a time.

use std::io::{self, Read};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use crate::scratch::Scratch;

/// The lifetime every worker a test starts is given, `--max-lifetime-s`: it ends by
/// itself after this, whatever happens to the test.
pub const LIFETIME_S: u32 = 120;

/// The entry point of the fakes: the worker with the pack's fake models, which takes a
/// fault (`pack/src/whizbang_pack/fakes.py`).
pub const FAKES: &str = "whizbang_pack.fakes";

/// How long `uv` has to find the Python.
const UV_LIMIT: Duration = Duration::from_secs(60);

/// The repository's `pack` folder.
fn pack_folder() -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../pack")).to_path_buf()
}

/// The pinned Python, found once in a process.
///
/// # Panics
/// When it is not found: a test that starts a worker cannot go on without it.
#[allow(clippy::panic)]
pub fn python() -> &'static Path {
    static FOUND: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    match FOUND.get_or_init(find_python) {
        Ok(path) => path,
        Err(why) => panic!("{why}"),
    }
}

/// The pack's source, which the supervisor puts on the Python path.
pub fn source() -> PathBuf {
    pack_folder().join("src")
}

fn find_python() -> Result<PathBuf, String> {
    let folder = pack_folder();
    let pinned = std::fs::read_to_string(folder.join(".python-version"))
        .map_err(|error| format!("pack/.python-version cannot be read: {error}"))?;
    let pinned = pinned.trim().to_owned();
    let install = format!(
        "install it once with: uv python install --no-config --no-bin --mirror \
         https://github.com/astral-sh/python-build-standalone/releases/download {pinned} \
         (that downloads it from GitHub's releases, and from nowhere else)"
    );
    let mut command = Command::new("uv");
    command.env_clear();
    for name in ["PATH", "HOME", "TMPDIR", "UV_PYTHON_INSTALL_DIR"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
        .envs([
            ("UV_NO_CONFIG", "1"),
            ("UV_OFFLINE", "1"),
            ("UV_PYTHON_DOWNLOADS", "never"),
            ("UV_MANAGED_PYTHON", "1"),
            ("UV_CONCURRENT_DOWNLOADS", "4"),
            ("UV_CONCURRENT_INSTALLS", "4"),
            ("UV_CONCURRENT_BUILDS", "1"),
        ])
        .args([
            "--no-config",
            "--offline",
            "--no-python-downloads",
            "--managed-python",
            "python",
            "find",
            &pinned,
        ])
        .current_dir(&folder)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|error| format!("uv cannot be started ({error}); the pack's tests need it"))?;
    let Some(mut answer) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err("uv's output was not piped".to_owned());
    };
    let (tell, told) = mpsc::channel();
    let reader = crate::threads::spawn(move || {
        let mut bytes = Vec::new();
        let read = answer.read_to_end(&mut bytes).map(|_| bytes);
        let _ = tell.send(read);
    });
    let read = told.recv_timeout(UV_LIMIT);
    if read.is_err() {
        // Its time is up: it is killed, which ends its output and the reader with it.
        let _ = child.kill();
    }
    let status = child.wait();
    let _ = reader.join();
    let bytes = match (read, status) {
        (Ok(Ok(bytes)), Ok(status)) if status.success() => bytes,
        (Err(_), _) => {
            return Err(format!(
                "uv did not find a Python within {} seconds; it was killed and waited for",
                UV_LIMIT.as_secs()
            ));
        }
        _ => {
            return Err(format!(
                "uv found no Python {pinned} that it manages; {install}"
            ));
        }
    };
    let path = PathBuf::from(String::from_utf8_lossy(&bytes).trim());
    if path.is_absolute() && path.is_file() {
        Ok(path)
    } else {
        Err(format!(
            "uv found no Python {pinned} that it manages; {install}"
        ))
    }
}

/// What a test starts: the fields of the supervisor's launch, and the test's own
/// models directory, which the guard keeps for as long as it lives.
#[derive(Debug)]
pub struct Fixture {
    /// The pinned Python.
    pub python: PathBuf,
    /// The pack's source.
    pub source: PathBuf,
    /// The entry point: [`FAKES`].
    pub module: String,
    /// The test's models directory.
    pub models_dir: PathBuf,
    /// [`LIFETIME_S`].
    pub lifetime_s: u32,
    /// The fault, as the entry point takes it, or nothing.
    pub more: Vec<String>,
    models: Scratch,
}

/// The pack with its fake models, and this fault of the worker's own
/// (`--fake-fault`), or none.
pub fn fakes(fault: Option<&str>) -> Fixture {
    let models = Scratch::new("pack-models");
    Fixture {
        python: python().to_path_buf(),
        source: source(),
        module: FAKES.to_owned(),
        models_dir: models.path().to_path_buf(),
        lifetime_s: LIFETIME_S,
        more: fault
            .map(|fault| vec!["--fake-fault".to_owned(), fault.to_owned()])
            .unwrap_or_default(),
        models,
    }
}

impl Fixture {
    /// The supervisor `make` makes from this fixture, held by a guard that ends its
    /// worker with `end` (a kill and a wait) when it is dropped.
    pub fn guard<T>(self, make: impl FnOnce(&Fixture) -> T, end: fn(&mut T)) -> Guard<T> {
        let held = make(&self);
        Guard {
            held: Some(held),
            end,
            _models: self.models,
        }
    }
}

/// A supervisor of a worker a test started. Its drop ends the worker, waits for it,
/// and fails the test if a child of the test's process still runs the pack then.
pub struct Guard<T> {
    held: Option<T>,
    end: fn(&mut T),
    _models: Scratch,
}

impl<T> Deref for Guard<T> {
    type Target = T;

    #[allow(clippy::expect_used)]
    fn deref(&self) -> &T {
        self.held
            .as_ref()
            .expect("the guard holds its supervisor until it is dropped")
    }
}

impl<T> DerefMut for Guard<T> {
    #[allow(clippy::expect_used)]
    fn deref_mut(&mut self) -> &mut T {
        self.held
            .as_mut()
            .expect("the guard holds its supervisor until it is dropped")
    }
}

impl<T> Drop for Guard<T> {
    #[allow(clippy::panic)]
    fn drop(&mut self) {
        if let Some(mut held) = self.held.take() {
            (self.end)(&mut held);
            drop(held);
        }
        let left = pack_children();
        if left.is_empty() {
            return;
        }
        let said = format!("a worker of the pack outlived its guard: processes {left:?}");
        if std::thread::panicking() {
            eprintln!("{said}");
        } else {
            panic!("{said}");
        }
    }
}

/// The children of this process that run the pack, by process id, as `ps` lists them
/// (a process whose command line names `whizbang_pack`). Nothing is signalled.
///
/// # Panics
/// When `ps` cannot be run: what it would have shown is not known.
#[allow(clippy::panic)]
pub fn pack_children() -> Vec<u32> {
    let own = std::process::id();
    pack_processes()
        .into_iter()
        .filter(|(_, parent)| *parent == own)
        .map(|(pid, _)| pid)
        .collect()
}

/// Whether a process with this id runs the pack, as `ps` lists the processes.
pub fn runs_the_pack(pid: u32) -> bool {
    pack_processes().iter().any(|(listed, _)| *listed == pid)
}

/// Every process of the machine that runs the pack: its id and its parent's.
#[allow(clippy::panic)]
fn pack_processes() -> Vec<(u32, u32)> {
    let listed = Command::new("ps")
        .args(["-A", "-o", "pid=,ppid=,command="])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();
    let listed = match listed {
        Ok(listed) if listed.status.success() => listed.stdout,
        other => panic!("ps could not list the processes: {other:?}"),
    };
    String::from_utf8_lossy(&listed)
        .lines()
        .filter(|line| line.contains("whizbang_pack"))
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let pid = words.next()?.parse().ok()?;
            let parent = words.next()?.parse().ok()?;
            Some((pid, parent))
        })
        .collect()
}

/// What the supervisor makes its two threads with in a test: [`crate::threads::spawn`],
/// so that they are counted with the test's other threads, and the one over the
/// process's limit is not made.
pub fn spawn(
    _name: String,
    work: Box<dyn FnOnce() + Send + 'static>,
) -> io::Result<JoinHandle<()>> {
    Ok(crate::threads::spawn(work))
}
