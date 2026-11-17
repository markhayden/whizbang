//! The one way a test starts the real binary: with nothing of the environment of whoever
//! runs the tests, and with a home that is a temporary folder.
//!
//! The real binary reads places that belong to a person: a home folder, the platform's
//! folders for data, settings and caches, the system's temporary folder. A test that
//! starts it with the environment the tests were run in would read, and could write,
//! the real ones. So a test starts it through [`command`] and no other way; `just
//! check` refuses any other (`scripts/check-binary-start.sh`).
//!
//! The binary's path is known only to the binary's own tests, which cargo gives it as
//! `env!("CARGO_BIN_EXE_whizbang")`, so the function is given it.

use std::ffi::OsStr;
use std::ops::{Deref, DerefMut};
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// How many names are tried for the temporary folder before the test fails.
const NAMES_TRIED: u32 = 64;

/// A shell found at the same place on macOS and on Linux, so that no `PATH` is searched.
const SHELL: &str = "/bin/bash";

/// A command for the real binary, and the temporary folder its environment names. It is
/// used as the `std::process::Command` it holds: a test adds its arguments, its
/// variables and its input, and starts it. The folder is removed when this is dropped,
/// so a test keeps it until the program it started has been waited for.
#[derive(Debug)]
pub struct Binary {
    // The command is dropped before the folder.
    command: Command,
    home: PathBuf,
    folder: Folder,
}

/// A folder that is removed, with what it holds, when this is dropped.
#[derive(Debug)]
struct Folder(PathBuf);

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Makes one folder, for its owner alone. It fails when the name is taken, by a folder,
/// a file or a link: a name somebody made first is never used.
fn make(folder: &Path) -> std::io::Result<()> {
    std::fs::DirBuilder::new().mode(0o700).create(folder)
}

/// A new folder in the system's temporary folder, for its owner alone.
///
/// # Panics
/// When no folder could be made: a test cannot go on without it.
#[allow(clippy::panic)]
fn folder() -> Folder {
    let within = std::env::temp_dir();
    let mut last = None;
    for _ in 0..NAMES_TRIED {
        let path = within.join(format!(
            "whizbang-binary-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match make(&path) {
            Ok(()) => return Folder(path),
            // A name that is taken is another's, and is left: the next is tried.
            Err(error) => last = Some(error),
        }
    }
    panic!(
        "cannot make a temporary folder in {}: {last:?}",
        within.display()
    );
}

/// The variables a started program is given, and no other: its home, the folders for
/// data, settings, caches and state inside that home, and a temporary folder beside it.
fn environment(command: &mut Command, folder: &Path, home: &Path) {
    command
        .env_clear()
        .env("HOME", home)
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("XDG_STATE_HOME", home.join(".local/state"))
        .env("TMPDIR", folder.join("tmp"));
}

/// The command that starts the binary at `path`, with no argument yet.
///
/// Its environment is cleared and then holds six variables, each a place inside one new
/// temporary folder that only its owner can read or write: `HOME`; `XDG_DATA_HOME`,
/// `XDG_CONFIG_HOME`, `XDG_CACHE_HOME` and `XDG_STATE_HOME`, inside that home;
/// and `TMPDIR`, beside it, so that what the binary keeps in the temporary folder is in
/// nobody else's. The home and the temporary folder exist; the others are made by
/// whoever writes to them. Nothing of the environment the tests run in is passed on: no
/// `PATH`, no token, no proxy. The binary needs none of it.
///
/// # Panics
/// When the temporary folder cannot be made: a test cannot go on without it.
#[allow(clippy::panic)]
pub fn command(path: impl AsRef<OsStr>) -> Binary {
    let folder = folder();
    let home = folder.0.join("home");
    for made in [home.clone(), folder.0.join("tmp")] {
        if let Err(error) = make(&made) {
            panic!("cannot make {}: {error}", made.display());
        }
    }
    let mut command = Command::new(path);
    environment(&mut command, &folder.0, &home);
    Binary {
        command,
        home,
        folder,
    }
}

impl Binary {
    /// The temporary home the started program is given.
    pub fn home(&self) -> &Path {
        &self.home
    }

    /// The same environment and the same folder, for a shell script that starts the
    /// binary itself: the command is `/bin/bash -c <script>`, and the script finds the
    /// binary's path in the variable `WHIZBANG`. For what only a shell can set up before
    /// the binary starts, such as a limit on the size of a file. Arguments given before
    /// this are dropped; the script names its own.
    #[must_use]
    pub fn under_shell(self, script: &str) -> Binary {
        let mut command = Command::new(SHELL);
        environment(&mut command, &self.folder.0, &self.home);
        command
            .env("WHIZBANG", self.command.get_program())
            .args(["-c", script]);
        Binary {
            command,
            home: self.home,
            folder: self.folder,
        }
    }
}

impl Deref for Binary {
    type Target = Command;

    fn deref(&self) -> &Command {
        &self.command
    }
}

impl DerefMut for Binary {
    fn deref_mut(&mut self) -> &mut Command {
        &mut self.command
    }
}
