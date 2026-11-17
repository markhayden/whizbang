//! The one function that starts the real binary: what the started program is given.
//!
//! The real binary is not built for this crate's tests, so the program started here is
//! `env`, which prints every variable it was started with: the whole environment is
//! read, not a sample of it.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use whizbang_testkit::binary;

/// A variable planted in the environment of the process that calls the function.
const PLANTED: &str = "WHIZBANG_TEST_PLANTED";
const PLANTED_VALUE: &str = "a-made-up-value-of-the-parent";
/// What a program that prints its environment is called on macOS and on Linux.
const ENV: &str = "/usr/bin/env";

/// What `env` printed, by name.
fn listed(out: &[u8]) -> BTreeMap<String, String> {
    String::from_utf8(out.to_vec())
        .expect("env prints UTF-8")
        .lines()
        .map(|line| {
            let (name, value) = line.split_once('=').expect("a name, `=`, a value");
            (name.to_owned(), value.to_owned())
        })
        .collect()
}

/// The folder the command's home is in: the temporary folder that is removed.
fn folder_of(home: &Path) -> PathBuf {
    home.parent().expect("the home has a folder").to_path_buf()
}

fn is_the_owners_alone(folder: &Path) -> bool {
    let mode = std::fs::metadata(folder).unwrap().permissions().mode();
    mode & 0o777 == 0o700
}

/// Run only by the test below, in a process of its own whose environment holds the
/// planted variable: it calls the function, and says `checked` when the program it
/// started saw what it should and the folder is gone.
#[test]
#[ignore = "run by the_command_has_a_cleared_environment_and_a_temporary_home"]
fn child_with_a_planted_variable_starts_the_command() {
    // This process has the planted variable, and a home and a path of its own.
    assert_eq!(std::env::var(PLANTED).as_deref(), Ok(PLANTED_VALUE));
    let own_home = std::env::var("HOME").expect("the test has a home");
    assert!(std::env::var_os("PATH").is_some());

    let mut command = binary::command(ENV);
    let home = command.home().to_path_buf();
    let folder = folder_of(&home);
    // The folder is this test's own: made for it, inside the system's temporary
    // folder, and nobody else's to read or write.
    assert!(folder.starts_with(std::env::temp_dir()), "{folder:?}");
    assert_ne!(folder, std::env::temp_dir());
    assert!(home.is_dir() && is_the_owners_alone(&folder) && is_the_owners_alone(&home));
    assert!(!home.starts_with(&own_home) && Path::new(&own_home) != home);

    // `output` waits for the program it started.
    let out = command.output().expect("env runs");
    assert!(out.status.success());
    let seen = listed(&out.stdout);
    // Every variable the program was given, and no other: nothing planted, no path, no
    // token, no proxy, nothing of this process.
    let names: Vec<&str> = seen.keys().map(String::as_str).collect();
    assert_eq!(
        names,
        [
            "HOME",
            "TMPDIR",
            "XDG_CACHE_HOME",
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_STATE_HOME"
        ],
        "{seen:?}"
    );
    assert!(!String::from_utf8_lossy(&out.stdout).contains(PLANTED_VALUE));
    // The home is the temporary one, and every folder named is inside the same
    // temporary folder; the two a program writes to without making them are there.
    assert_eq!(Path::new(&seen["HOME"]), home);
    for (name, value) in &seen {
        assert!(Path::new(value).starts_with(&folder), "{name}={value}");
    }
    for name in [
        "XDG_CACHE_HOME",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_STATE_HOME",
    ] {
        assert!(Path::new(&seen[name]).starts_with(&home), "{name}");
    }
    let temporary = Path::new(&seen["TMPDIR"]);
    assert!(temporary.is_dir() && is_the_owners_alone(temporary));
    assert!(!temporary.starts_with(&home));

    // A variable the test adds is the test's to add, and is seen.
    let out = command.env("WHIZBANG_TEST_ADDED", "1").output().unwrap();
    assert_eq!(listed(&out.stdout).len(), 7);

    // Through a shell, the same environment and the binary's path in `WHIZBANG`.
    let script = binary::command(ENV).under_shell("exec \"$WHIZBANG\"");
    let shell_home = script.home().to_path_buf();
    let mut script = script;
    let out = script.output().expect("the shell runs");
    assert!(out.status.success());
    let seen_under_shell = listed(&out.stdout);
    assert_eq!(seen_under_shell["WHIZBANG"], ENV);
    assert_eq!(Path::new(&seen_under_shell["HOME"]), shell_home);
    assert!(!seen_under_shell.contains_key(PLANTED));
    // Beside those, only what the shell sets for itself: the folder it was started
    // in, which is this test's and is no variable handed on, a depth and its own name.
    for (name, value) in &seen_under_shell {
        let the_shells_own = ["PWD", "OLDPWD", "SHLVL", "_"].contains(&name.as_str());
        assert!(
            seen.contains_key(name) || name == "WHIZBANG" || the_shells_own,
            "{name}={value}"
        );
        assert!(
            the_shells_own || !value.contains(&own_home),
            "{name}={value}"
        );
    }
    drop(script);
    assert!(!folder_of(&shell_home).exists());

    // The folder goes with the command.
    assert!(folder.exists());
    drop(command);
    assert!(!folder.exists());
    println!("\nchecked");
}

#[test]
fn the_command_has_a_cleared_environment_and_a_temporary_home() {
    // A variable cannot be planted in this process's own environment without code
    // this workspace forbids, so the test binary runs itself once, for the one test
    // above, with the variable planted: that process is the one that calls the
    // function. It is started once, by no loop, runs one test on one thread, which
    // starts three programs in turn and waits for each; `output` waits for it.
    let out = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "binary::child_with_a_planted_variable_starts_the_command",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(PLANTED, PLANTED_VALUE)
        .output()
        .expect("the test binary runs");
    let said = String::from_utf8_lossy(&out.stdout);
    let complaint = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{said}\n{complaint}");
    assert!(said.lines().any(|line| line == "checked"), "{said}");
}

#[test]
fn two_commands_at_once_have_a_folder_each() {
    let first = binary::command(ENV);
    let second = binary::command(ENV);
    assert_ne!(first.home(), second.home());
    assert!(first.home().is_dir() && second.home().is_dir());
}
