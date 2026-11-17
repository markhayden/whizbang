//! `whizbang setup`: the Python pack installed into an environment whizbang owns, from
//! the lock file this build carries, by `uv` at its pinned version, and a check that the
//! worker it made says hello (plan 1C, D9, D10 and task Y1; ADR 0021).
//!
//! **What is installed.** The pack this build carries ([`Pack::carried`]): the pack's
//! `pyproject.toml`, its `uv.lock`, its `.python-version` and every module of its
//! source, compiled into the binary from the repository's `pack` folder. Nothing of the
//! pack is read from a path at run time: the worker the kind `pack` starts runs the copy
//! `setup` wrote ([`at`]).
//!
//! **Where.** In the models directory, beside the models' own folder: `pack/project`
//! holds the copy of the pack, `pack/env` the environment `uv` made, with its Python.
//! One models directory, one pack; a second `setup` with the same pack and the same
//! extra changes nothing.
//!
//! **How**, in this order, each step past the first only when the one before ended
//! well:
//!
//! 1. The pack is written into a folder of `setup`'s own in the system's temporary
//!    folder, for its owner alone, removed on every path out.
//! 2. `uv --version` must say the pinned version (`scripts/tool-versions.txt`).
//! 3. `uv`'s `lock --check`, offline: the lock must match the pack's own file. A lock
//!    that does not is refused here, and nothing in the models directory was touched.
//! 4. `uv`'s `sync --locked --no-build`, with the extra if one was named, into `pack/env`:
//!    offline with no extra, which installs no package; with one, from the package index
//!    the lock names and no other (`pypi.org`, its files at `files.pythonhosted.org`).
//! 5. The copy of the pack in `pack/project` is brought to what was installed: a file
//!    is written only where it differs, by a new file renamed over the old.
//! 6. The worker is started from the environment, with a lifetime of 120 seconds of its
//!    own (`--max-lifetime-s`; no config is read for it), says hello, and is stopped
//!    and waited for.
//!
//! **Every `uv`** (plan 1C, "Rules", the row on `uv`) is started with a cleared
//! environment: the locale of [`whizbang_backends::pack::worker::PASSED`], but for a
//! key's, and then only what `setup` sets ([`UV_NAMES`]); with no user's configuration
//! (`--no-config`), no cache of its own (`--no-cache`: a temporary one, in `setup`'s
//! folder), no download of a Python, no build of a package, and the three bounds on
//! what it does at once. It has 60 seconds, or 30 minutes for an install with an extra,
//! and is killed and waited for past them, and on every other path out of `setup`, a
//! panic among them. Its output goes to files in `setup`'s folder, read afterwards, so
//! no thread is made for it.

use std::ffi::{OsStr, OsString};
use std::io::{self, Read, Seek, SeekFrom};
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use whizbang_backends::pack::worker::{PASSED, is_key};
use whizbang_backends::pack::{Launch, Said, Worker};
use whizbang_core::config::Loaded;
use whizbang_core::contract::{Secrets, SystemClock};

use crate::wiring::{self, Models, Platform};

/// The lifetime the worker `setup` starts for its hello is given (`--max-lifetime-s`):
/// `setup` is built before the kind `pack` and its setting, so it is fixed here.
pub const HELLO_LIFETIME_S: u32 = 120;

/// How long a `uv` with nothing to download has: its version, the check of the lock,
/// and an install with no extra.
pub const QUICK_LIMIT: Duration = Duration::from_secs(60);

/// How long the install of an extra has, which downloads the runtime.
pub const EXTRA_LIMIT: Duration = Duration::from_secs(30 * 60);

/// What is let pass between two looks at a `uv` that runs.
const PACE: Duration = Duration::from_millis(20);

/// The most bytes of the end of `uv`'s error output that a complaint quotes, and the
/// most lines.
const SAID_BYTES: u64 = 4_096;
const SAID_LINES: usize = 8;

/// The tools `just check` needs, at their pins; the line of `uv` is the version `setup`
/// asks for.
const TOOL_VERSIONS: &str = include_str!("../../../scripts/tool-versions.txt");

/// The names of every variable `setup` may give `uv`, and no other: the locale of the
/// worker's allow-list, then what `setup` sets itself.
pub const UV_NAMES: [&str; 16] = [
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "PATH",
    "TMPDIR",
    "UV_NO_CONFIG",
    "UV_NO_CACHE",
    "UV_NO_BUILD",
    "UV_PYTHON_DOWNLOADS",
    "UV_CONCURRENT_DOWNLOADS",
    "UV_CONCURRENT_INSTALLS",
    "UV_CONCURRENT_BUILDS",
    "UV_OFFLINE",
    "UV_PROJECT_ENVIRONMENT",
    "UV_PYTHON_INSTALL_DIR",
    "UV_MANAGED_PYTHON",
];

/// How long each `uv` has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Its version, the check of the lock, and an install with no extra.
    pub quick: Duration,
    /// An install with an extra.
    pub extra: Duration,
}

impl Limits {
    /// What the binary gives: [`QUICK_LIMIT`] and [`EXTRA_LIMIT`].
    pub fn shipped() -> Limits {
        Limits {
            quick: QUICK_LIMIT,
            extra: EXTRA_LIMIT,
        }
    }
}

/// The files of the pack this build carries, by their place in the pack's folder.
const CARRIED: [(&str, &[u8]); 9] = [
    (
        "pyproject.toml",
        include_bytes!("../../../pack/pyproject.toml"),
    ),
    ("uv.lock", include_bytes!("../../../pack/uv.lock")),
    (
        ".python-version",
        include_bytes!("../../../pack/.python-version"),
    ),
    (
        "src/whizbang_pack/__init__.py",
        include_bytes!("../../../pack/src/whizbang_pack/__init__.py"),
    ),
    (
        "src/whizbang_pack/adapter.py",
        include_bytes!("../../../pack/src/whizbang_pack/adapter.py"),
    ),
    (
        "src/whizbang_pack/fake_models.py",
        include_bytes!("../../../pack/src/whizbang_pack/fake_models.py"),
    ),
    (
        "src/whizbang_pack/fakes.py",
        include_bytes!("../../../pack/src/whizbang_pack/fakes.py"),
    ),
    (
        "src/whizbang_pack/proto.py",
        include_bytes!("../../../pack/src/whizbang_pack/proto.py"),
    ),
    (
        "src/whizbang_pack/worker.py",
        include_bytes!("../../../pack/src/whizbang_pack/worker.py"),
    ),
];

/// The pack's files, each by its place in the pack's folder (`src/whizbang_pack/…`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pack {
    files: Vec<(String, Vec<u8>)>,
}

impl Pack {
    /// The pack this build carries: the repository's, when it was built.
    pub fn carried() -> Pack {
        Pack {
            files: CARRIED
                .iter()
                .map(|(name, bytes)| ((*name).to_owned(), bytes.to_vec()))
                .collect(),
        }
    }

    /// The same, with this file in place of the one at its place, or beside them: what
    /// a test gives to make a pack whose lock does not match.
    #[must_use]
    pub fn with_file(mut self, path: &str, bytes: impl Into<Vec<u8>>) -> Pack {
        let bytes = bytes.into();
        match self.files.iter_mut().find(|(name, _)| name == path) {
            Some(file) => file.1 = bytes,
            None => self.files.push((path.to_owned(), bytes)),
        }
        self
    }

    /// Its files.
    pub fn files(&self) -> &[(String, Vec<u8>)] {
        &self.files
    }
}

/// Where the pack of a models directory is: what `setup` makes and the kind `pack`
/// starts its worker from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    /// `pack/` in the models directory: everything `setup` makes is in it.
    pub folder: PathBuf,
    /// `pack/project`: the copy of the pack that was installed.
    pub project: PathBuf,
    /// `pack/env`: the environment `uv` made.
    pub environment: PathBuf,
    /// The environment's Python.
    pub python: PathBuf,
    /// The pack's source, put on the worker's Python path.
    pub source: PathBuf,
}

/// Where the pack of the models directory at `root` is.
pub fn at(root: &Path) -> Place {
    let folder = root.join("pack");
    let project = folder.join("project");
    let environment = folder.join("env");
    Place {
        python: environment.join("bin").join("python"),
        source: project.join("src"),
        folder,
        project,
        environment,
    }
}

/// What `setup` starts for its hello: the worker itself, from the environment of this
/// place, told the models directory, with a lifetime of [`HELLO_LIFETIME_S`].
pub fn hello_launch(place: &Place, root: &Path) -> Launch {
    Launch::worker(
        place.python.clone(),
        place.source.clone(),
        root.to_path_buf(),
        HELLO_LIFETIME_S,
    )
}

/// What one install is given.
#[derive(Debug, Clone)]
pub struct Install {
    /// The models directory: opened already, as the binary's commands open one.
    pub root: PathBuf,
    /// `uv`, by an absolute path.
    pub uv: PathBuf,
    /// The Python the environment is made with, by an absolute path; with none, `uv`
    /// finds the version the pack pins among the Pythons it manages.
    pub python: Option<PathBuf>,
    /// The extra to install with the pack's core, or none.
    pub extra: Option<String>,
    /// The pack.
    pub pack: Pack,
    /// How long each `uv` has.
    pub limits: Limits,
    /// The environment `uv`'s is chosen from: the locale, and where `uv` keeps the
    /// Pythons it manages. Nothing else of it is handed on.
    pub parent: Vec<(OsString, OsString)>,
}

/// What an install made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    /// Where.
    pub place: Place,
    /// What the worker said of itself in its hello.
    pub said: Said,
}

/// Installs the pack into the models directory and checks that its worker says hello.
/// Why not, otherwise, in fixed words, with the end of what `uv` said where it said
/// something.
pub fn install(install: &Install) -> Result<Installed, String> {
    if let Some(extra) = &install.extra
        && !is_extra_name(extra)
    {
        return Err("an extra's name is lower-case letters, digits and '-', 64 at most, starting with a letter; nothing was changed".to_owned());
    }
    if !install.uv.is_absolute() || !install.uv.is_file() {
        return Err(
            "uv must be named by an absolute path to a file; nothing was changed".to_owned(),
        );
    }
    if let Some(python) = &install.python
        && (!python.is_absolute() || !python.is_file())
    {
        return Err(
            "the Python must be named by an absolute path to a file; nothing was changed"
                .to_owned(),
        );
    }
    let work = Work::make()
        .map_err(|_| "setup could not make its folder; nothing was changed".to_owned())?;
    let staged = work.path.join("project");
    write_pack(&install.pack, &staged).map_err(|_| {
        "setup could not write the pack into its folder; nothing was changed".to_owned()
    })?;
    let place = at(&install.root);
    let uv = Uv {
        program: &install.uv,
        work: &work,
        project: &staged,
        place: &place,
        install,
    };

    let version = uv.run(&["--version"], false, install.limits.quick)?;
    let pinned = pinned_uv();
    if !version.status.success() || !said_version(&version.out, &pinned) {
        return Err(format!(
            "the uv named is not the pinned version, {pinned} (scripts/tool-versions.txt); nothing was changed"
        ));
    }

    let check = uv.run(&["lock", "--check"], false, install.limits.quick)?;
    if !check.status.success() {
        return Err(said_too(
            "the lock file does not match the pack, or uv could not check it offline; nothing was changed",
            &check.err,
        ));
    }

    make_folder(&place.folder).map_err(|_| {
        "the folder of the pack in the models directory could not be made".to_owned()
    })?;
    let mut sync: Vec<String> = ["sync", "--locked", "--no-build"]
        .iter()
        .map(|arg| (*arg).to_owned())
        .collect();
    let (online, limit) = match &install.extra {
        Some(extra) => {
            sync.push(format!("--extra={extra}"));
            (true, install.limits.extra)
        }
        None => (false, install.limits.quick),
    };
    let sync: Vec<&str> = sync.iter().map(String::as_str).collect();
    let synced = uv.run(&sync, online, limit)?;
    if !synced.status.success() {
        return Err(said_too(
            "uv could not install the pack from its lock file",
            &synced.err,
        ));
    }

    bring(&install.pack, &place.project).map_err(|_| {
        "the copy of the pack in the models directory could not be written".to_owned()
    })?;

    let mut worker = Worker::new(
        hello_launch(&place, &install.root),
        Arc::new(SystemClock),
        Secrets::none(),
    );
    let ready = worker.ready();
    let said = worker.said().cloned();
    worker.stop();
    match (ready, said) {
        (Ok(()), Some(said)) => Ok(Installed { place, said }),
        (Err(fault), _) => Err(format!(
            "the pack was installed, but the worker it made did not say hello: {}",
            fault.sentence()
        )),
        (Ok(()), None) => {
            Err("the pack was installed, but the worker it made did not say hello".to_owned())
        }
    }
}

/// Whether a text is an extra's name as the pack writes them.
fn is_extra_name(name: &str) -> bool {
    name.len() <= 64
        && name.bytes().next().is_some_and(|b| b.is_ascii_lowercase())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// The version of `uv` the tools are pinned to.
fn pinned_uv() -> String {
    TOOL_VERSIONS
        .lines()
        .find_map(|line| line.strip_prefix("uv "))
        .map(|version| version.trim().to_owned())
        .unwrap_or_default()
}

/// Whether `uv --version` said this version: `uv <version>`, then a space or the end.
fn said_version(out: &str, pinned: &str) -> bool {
    !pinned.is_empty()
        && out.lines().next().is_some_and(|line| {
            line.strip_prefix("uv ")
                .and_then(|rest| rest.strip_prefix(pinned))
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '))
        })
}

/// A complaint, and the end of what `uv` said on its error output, a line each.
fn said_too(complaint: &str, err: &str) -> String {
    let lines: Vec<&str> = err.lines().filter(|line| !line.trim().is_empty()).collect();
    if lines.is_empty() {
        return complaint.to_owned();
    }
    let from = lines.len().saturating_sub(SAID_LINES);
    format!("{complaint}; uv said:\n{}", lines[from..].join("\n"))
}

/// The folder `setup` works in: in the system's temporary folder, for its owner alone,
/// removed with what it holds when this is dropped, on every path out.
struct Work {
    path: PathBuf,
}

impl Work {
    fn make() -> io::Result<Work> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let base = std::env::temp_dir();
        let mut last = io::Error::other("no folder was tried");
        for _ in 0..16 {
            let path = base.join(format!(
                "whizbang-setup-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => {
                    let work = Work { path };
                    make_folder(&work.path.join("tmp"))?;
                    return Ok(work);
                }
                Err(error) => last = error,
            }
        }
        Err(last)
    }
}

impl Drop for Work {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Makes a folder and the folders above it, each made here for its owner alone.
fn make_folder(path: &Path) -> io::Result<()> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
}

/// A file of the pack, by its place, as a path under `folder`: one plain name after
/// another, nothing that climbs or starts at the root.
fn under(folder: &Path, name: &str) -> io::Result<PathBuf> {
    let mut path = folder.to_path_buf();
    for part in name.split('/') {
        if part.is_empty() || part == "." || part == ".." || part.contains('\\') {
            return Err(io::Error::other("a file of the pack has no plain name"));
        }
        path.push(part);
    }
    Ok(path)
}

/// Writes the pack into a new folder.
fn write_pack(pack: &Pack, folder: &Path) -> io::Result<()> {
    for (name, bytes) in pack.files() {
        let path = under(folder, name)?;
        if let Some(parent) = path.parent() {
            make_folder(parent)?;
        }
        std::fs::write(path, bytes)?;
    }
    Ok(())
}

/// Brings the copy of the pack in `folder` to `pack`: each file that differs is
/// written, as a new file renamed over the old, and a module of the pack's source that
/// the pack no longer has is removed. A file that is the same is not touched.
fn bring(pack: &Pack, folder: &Path) -> io::Result<()> {
    for (name, bytes) in pack.files() {
        let path = under(folder, name)?;
        if std::fs::read(&path).ok().as_deref() == Some(bytes.as_slice()) {
            continue;
        }
        if let Some(parent) = path.parent() {
            make_folder(parent)?;
        }
        let mut new = path.clone().into_os_string();
        new.push(".new");
        std::fs::write(&new, bytes)?;
        std::fs::rename(&new, &path)?;
    }
    let package = folder.join("src").join("whizbang_pack");
    if let Ok(listing) = std::fs::read_dir(&package) {
        for entry in listing.take(4_096) {
            let entry = entry?;
            let name = format!("src/whizbang_pack/{}", entry.file_name().to_string_lossy());
            if entry.file_type()?.is_file() && !pack.files().iter().any(|(had, _)| *had == name) {
                std::fs::remove_file(entry.path())?;
            }
        }
    }
    Ok(())
}

/// How one `uv` ended, and the ends of what it said.
struct Ran {
    status: ExitStatus,
    out: String,
    err: String,
}

/// What every `uv` of one install is started with.
struct Uv<'a> {
    program: &'a Path,
    work: &'a Work,
    project: &'a Path,
    place: &'a Place,
    install: &'a Install,
}

impl Uv<'_> {
    /// Runs `uv` with these arguments after the ones every `uv` is given, in the staged
    /// pack, online only when `online`; within `limit`, past which it is killed and
    /// waited for and this is the complaint.
    fn run(&self, args: &[&str], online: bool, limit: Duration) -> Result<Ran, String> {
        let out_file = self.work.path.join("uv.out");
        let err_file = self.work.path.join("uv.err");
        let files = std::fs::File::create(&out_file)
            .and_then(|out| Ok((out, std::fs::File::create(&err_file)?)))
            .map_err(|_| "setup could not make the files uv writes to".to_owned())?;
        let mut command = Command::new(self.program);
        command.args(["--no-config", "--no-cache", "--no-python-downloads"]);
        if !online {
            command.arg("--offline");
        }
        command.args(args);
        if args.first().is_some_and(|first| *first != "--version") {
            match &self.install.python {
                Some(python) => {
                    command.arg("--python").arg(python);
                }
                None => {
                    command.arg("--managed-python");
                }
            }
        }
        command
            .env_clear()
            .envs(uv_environment(
                &self.install.parent,
                &self.work.path.join("tmp"),
                &self.place.environment,
                online,
                self.install.python.is_none(),
            ))
            .current_dir(self.project)
            .stdin(Stdio::null())
            .stdout(files.0)
            .stderr(files.1);
        let child = command
            .spawn()
            .map_err(|_| "uv could not be started; nothing was changed".to_owned())?;
        let mut running = Running { child };
        let status = running.wait_within(limit).ok_or_else(|| {
            format!(
                "uv did not end within {} seconds; it was killed and waited for",
                limit.as_secs()
            )
        })?;
        Ok(Ran {
            status,
            out: tail(&out_file),
            err: tail(&err_file),
        })
    }
}

/// The end of a file `uv` wrote, at most [`SAID_BYTES`], as text.
fn tail(path: &Path) -> String {
    let Ok(mut file) = std::fs::File::open(path) else {
        return String::new();
    };
    let length = file.metadata().map(|meta| meta.len()).unwrap_or(0);
    let _ = file.seek(SeekFrom::Start(length.saturating_sub(SAID_BYTES)));
    let mut bytes = Vec::new();
    let _ = file.take(SAID_BYTES).read_to_end(&mut bytes);
    String::from_utf8_lossy(&bytes).into_owned()
}

/// A `uv` that was started: killed and waited for when this is dropped, unless it was
/// waited for already, on every path out, a panic among them.
struct Running {
    child: Child,
}

impl Running {
    /// Waits for it within `limit`, looking every [`PACE`] and at most a bounded count
    /// of times; past it, it is killed and waited for, and nothing is said.
    fn wait_within(&mut self, limit: Duration) -> Option<ExitStatus> {
        let until = Instant::now() + limit;
        let most = 2 * (limit.as_millis() / PACE.as_millis()).max(1) + 1;
        for _ in 0..most {
            match self.child.try_wait() {
                Ok(Some(status)) => return Some(status),
                Ok(None) if Instant::now() < until => std::thread::sleep(PACE),
                _ => break,
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        None
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if !matches!(self.child.try_wait(), Ok(Some(_))) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// The environment of one `uv`: the parent's locale, but for a key's name, then what
/// `setup` sets. `tmp` is `setup`'s own temporary folder, `environment` the folder of
/// the environment to make, `online` whether this `uv` may reach the package index, and
/// `managed` whether it finds the Python itself among those it manages, in the folder
/// it keeps them in: the parent's `UV_PYTHON_INSTALL_DIR`, or else its default under
/// the parent's data folder.
pub fn uv_environment(
    parent: &[(OsString, OsString)],
    tmp: &Path,
    environment: &Path,
    online: bool,
    managed: bool,
) -> Vec<(OsString, OsString)> {
    let mut chosen: Vec<(OsString, OsString)> = parent
        .iter()
        .filter(|(name, _)| {
            let name = name.to_string_lossy();
            PASSED.contains(&name.as_ref()) && !is_key(&name, &[])
        })
        .cloned()
        .collect();
    let mut set: Vec<(&str, OsString)> = vec![
        ("PATH", "/usr/bin:/bin".into()),
        ("TMPDIR", tmp.as_os_str().to_owned()),
        ("UV_NO_CONFIG", "1".into()),
        ("UV_NO_CACHE", "1".into()),
        ("UV_NO_BUILD", "1".into()),
        ("UV_PYTHON_DOWNLOADS", "never".into()),
        ("UV_CONCURRENT_DOWNLOADS", "4".into()),
        ("UV_CONCURRENT_INSTALLS", "4".into()),
        ("UV_CONCURRENT_BUILDS", "1".into()),
        ("UV_PROJECT_ENVIRONMENT", environment.as_os_str().to_owned()),
    ];
    if !online {
        set.push(("UV_OFFLINE", "1".into()));
    }
    if managed {
        set.push(("UV_MANAGED_PYTHON", "1".into()));
        if let Some(pythons) = managed_pythons(parent) {
            set.push(("UV_PYTHON_INSTALL_DIR", pythons.into_os_string()));
        }
    }
    for (name, value) in set {
        chosen.retain(|(had, _)| had != name);
        chosen.push((name.into(), value));
    }
    chosen
}

/// Where `uv` keeps the Pythons it manages, as the parent's environment says: its
/// `UV_PYTHON_INSTALL_DIR`, or `uv/python` in its data folder (`XDG_DATA_HOME`, or
/// `.local/share` in its home). Only an absolute path names a place.
fn managed_pythons(parent: &[(OsString, OsString)]) -> Option<PathBuf> {
    let get = |name: &str| -> Option<PathBuf> {
        parent
            .iter()
            .find(|(had, _)| had == name)
            .map(|(_, value)| PathBuf::from(value))
            .filter(|path| path.is_absolute())
    };
    get("UV_PYTHON_INSTALL_DIR")
        .or_else(|| get("XDG_DATA_HOME").map(|data| data.join("uv").join("python")))
        .or_else(|| {
            get("HOME").map(|home| home.join(".local").join("share").join("uv").join("python"))
        })
}

/// What `whizbang setup` was asked.
#[derive(Debug, Clone, Copy)]
pub struct Asked<'a> {
    /// The config, for where it says the models directory is.
    pub config: Option<&'a Path>,
    /// Its env file.
    pub env_file: Option<&'a Path>,
    /// `--models-dir`.
    pub models_dir: Option<&'a Path>,
    /// `--extra`.
    pub extra: Option<&'a str>,
    /// `--uv`.
    pub uv: Option<&'a Path>,
    /// `--python`.
    pub python: Option<&'a Path>,
}

/// What the machine says, as `main` reads it: where the platform's models directory
/// is (asked only when nothing else names one), the `PATH` that `uv` is looked for on
/// when `--uv` names none, and the environment `uv`'s is chosen from.
pub struct Machine {
    pub platform: Platform,
    pub path: Option<OsString>,
    pub parent: Vec<(OsString, OsString)>,
}

/// Runs `whizbang setup`: what to print when it ended well, or the complaint.
pub fn run(asked: Asked<'_>, machine: Machine) -> Result<String, String> {
    let models = Models::shipped()?
        .flag(asked.models_dir)
        .platform(machine.platform);
    models.flag_is_absolute()?;
    let config_dir = match asked.config {
        Some(config) => {
            let loaded = Loaded::of_files(config, asked.env_file)
                .map_err(|refused| format!("the config cannot be used: {refused}"))?;
            loaded.config().models_dir().map(Path::to_owned)
        }
        None => None,
    };
    let uv = match asked.uv {
        Some(uv) => uv.to_path_buf(),
        None => on_path(machine.path.as_deref(), "uv").ok_or_else(|| {
            format!(
                "uv was not found on PATH; install uv {} (scripts/tool-versions.txt), or name it with --uv",
                pinned_uv()
            )
        })?,
    };
    let (said, root) = models.chosen(config_dir.as_deref())?;
    wiring::open_models_dir(said, &root, Arc::new(SystemClock))?;
    let installed = install(&Install {
        root: root.clone(),
        uv,
        python: asked.python.map(Path::to_path_buf),
        extra: asked.extra.map(str::to_owned),
        pack: Pack::carried(),
        limits: Limits::shipped(),
        parent: machine.parent,
    })?;
    Ok(format!(
        "the pack is installed in {}\nextra: {}\nthe worker it made said hello (lifetime {} s): device {}, {} threads\n",
        installed.place.folder.display(),
        asked.extra.unwrap_or("none"),
        HELLO_LIFETIME_S,
        installed.said.device.as_deref().unwrap_or("none"),
        installed.said.threads,
    ))
}

/// The first file of this name in an absolute folder of `PATH`.
fn on_path(path: Option<&OsStr>, name: &str) -> Option<PathBuf> {
    std::env::split_paths(path?)
        .filter(|folder| folder.is_absolute())
        .map(|folder| folder.join(name))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_said_is_the_pinned_one_and_no_other() {
        assert!(said_version(
            "uv 0.12.23 (46b84fd0b 2026-10-03 aarch64-apple-darwin)\n",
            "0.12.23"
        ));
        assert!(said_version("uv 0.12.23\n", "0.12.23"));
        assert!(!said_version("uv 0.12.230\n", "0.12.23"));
        assert!(!said_version("uv 0.12.2\n", "0.12.23"));
        assert!(!said_version("", "0.12.23"));
        assert!(!said_version("uv 0.12.23", ""));
        assert_eq!(pinned_uv(), "0.12.23");
    }

    #[test]
    fn a_file_of_the_pack_is_written_only_under_its_folder() {
        let folder = Path::new("/a/b");
        assert_eq!(
            under(folder, "src/whizbang_pack/x.py").unwrap(),
            Path::new("/a/b/src/whizbang_pack/x.py")
        );
        for bad in ["", "/x", "../x", "a/../../x", "a//b", "./x", "a\\b"] {
            assert!(under(folder, bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn an_extras_name_is_plain() {
        for good in ["torch", "a", "a-1"] {
            assert!(is_extra_name(good), "{good}");
        }
        for bad in ["", "-x", "Torch", "1a", "a b", "a=b", &"a".repeat(65)] {
            assert!(!is_extra_name(bad), "{bad}");
        }
    }

    #[test]
    fn the_environment_names_only_what_the_list_holds() {
        let parent: Vec<(OsString, OsString)> = [
            ("LANG", "C"),
            ("LC_ALL", "C"),
            ("HOME", "/h"),
            ("OPENAI_API_KEY", "x"),
            ("UV_INDEX_URL", "https://x"),
        ]
        .iter()
        .map(|(n, v)| ((*n).into(), (*v).into()))
        .collect();
        for (online, managed) in [(false, false), (true, false), (false, true), (true, true)] {
            let set = uv_environment(&parent, Path::new("/t"), Path::new("/e"), online, managed);
            for (name, _) in &set {
                assert!(
                    UV_NAMES.contains(&name.to_string_lossy().as_ref()),
                    "{name:?}"
                );
            }
            let has = |name: &str| set.iter().any(|(had, _)| had == name);
            assert_eq!(has("UV_OFFLINE"), !online);
            assert_eq!(has("UV_MANAGED_PYTHON"), managed);
            assert_eq!(has("UV_PYTHON_INSTALL_DIR"), managed);
            assert!(has("LANG") && has("LC_ALL") && !has("HOME"));
        }
        let managed = uv_environment(&parent, Path::new("/t"), Path::new("/e"), false, true);
        assert!(
            managed
                .iter()
                .any(|(name, value)| name == "UV_PYTHON_INSTALL_DIR"
                    && value == "/h/.local/share/uv/python")
        );
    }
}
