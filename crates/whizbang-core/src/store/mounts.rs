//! Which file system a store is on, from the system's own table of mounts (plan m6, task
//! S1, item 4). A store lives on a local file system: on a network one its locks may hold
//! on one machine alone, and its readers' holds and prune's lock differ ([`super::fs`]).
//! So a store on a mount of a network type is refused when it is opened.
//!
//! The table is read as text, with nothing beyond the standard library: on Linux the
//! file `/proc/self/mountinfo`, on macOS the output of `/sbin/mount`. A file system that
//! holds locks on one machine alone and is not of a type named here is not found by this;
//! the lock asked a second time is the other check ([`super::Store::open`]).

use std::path::{Path, PathBuf};

/// The types of a network file system, as the two tables name them. Any `fuse.` type
/// (Linux) or `fuse` one (macOS) is refused too: the table names the program behind it,
/// not whether its locks hold.
pub const NETWORK_TYPES: [&str; 10] = [
    "nfs",
    "nfs4",
    "cifs",
    "smbfs",
    "afpfs",
    "webdav",
    "9p",
    "ceph",
    "glusterfs",
    "fuse.sshfs",
];

/// Whether a file system of this type is a network one, by its name.
pub fn is_network(kind: &str) -> bool {
    NETWORK_TYPES.contains(&kind)
        || kind.starts_with("fuse.")
        || kind == "fuse"
        || kind.starts_with("macfuse")
        || kind.starts_with("osxfuse")
}

/// The mounts of a Linux table (`/proc/self/mountinfo`), each as its mount point and its
/// type. A line not of the table's shape is passed over.
pub fn of_mountinfo(table: &str) -> Vec<(PathBuf, String)> {
    table
        .lines()
        .filter_map(|line| {
            // `id parent major:minor root point options [optional...] - type source more`
            let (before, after) = line.split_once(" - ")?;
            let point = before.split(' ').nth(4)?;
            let kind = after.split(' ').next()?;
            Some((PathBuf::from(unescape(point)), kind.to_owned()))
        })
        .collect()
}

/// The mounts of a macOS table (the output of `/sbin/mount`), each as its mount point and
/// its type: `<device> on <point> (<type>, <options>...)`.
pub fn of_mount_output(table: &str) -> Vec<(PathBuf, String)> {
    table
        .lines()
        .filter_map(|line| {
            let (_, rest) = line.split_once(" on ")?;
            let open = rest.rfind(" (")?;
            let (point, options) = (&rest[..open], &rest[open + 2..]);
            let kind = options.split([',', ')']).next()?.trim();
            Some((PathBuf::from(point), kind.to_owned()))
        })
        .collect()
}

/// The type of the mount that holds `path`: the one whose point is the longest that
/// `path` is under. `None` when no mount holds it.
pub fn type_of(mounts: &[(PathBuf, String)], path: &Path) -> Option<String> {
    mounts
        .iter()
        .filter(|(point, _)| path.starts_with(point))
        .max_by_key(|(point, _)| point.components().count())
        .map(|(_, kind)| kind.clone())
}

/// The `\040`-style escapes of a mount point in `mountinfo`: a space, a tab, a newline
/// and a backslash.
fn unescape(point: &str) -> String {
    let mut out = String::with_capacity(point.len());
    let mut rest = point;
    while let Some(at) = rest.find('\\') {
        out.push_str(&rest[..at]);
        let code = rest.get(at + 1..at + 4);
        match code.and_then(|code| u8::from_str_radix(code, 8).ok()) {
            Some(byte) => {
                out.push(byte as char);
                rest = &rest[at + 4..];
            }
            None => {
                out.push('\\');
                rest = &rest[at + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// This system's table of mounts, read once a process: a mount made or unmade after the
/// first store this process opened is not seen. Why it could not be read, otherwise.
pub(super) fn this_system() -> Result<Vec<(PathBuf, String)>, String> {
    static READ: std::sync::OnceLock<Result<Vec<(PathBuf, String)>, String>> =
        std::sync::OnceLock::new();
    READ.get_or_init(read_table).clone()
}

#[cfg(target_os = "linux")]
fn read_table() -> Result<Vec<(PathBuf, String)>, String> {
    let table = std::fs::read_to_string("/proc/self/mountinfo")
        .map_err(|error| format!("the table of mounts could not be read: {error}"))?;
    Ok(of_mountinfo(&table))
}

#[cfg(target_os = "macos")]
fn read_table() -> Result<Vec<(PathBuf, String)>, String> {
    // A cleared environment and the program by its full path: nothing of the caller's
    // is handed to it, and it is waited for.
    let output = std::process::Command::new("/sbin/mount")
        .env_clear()
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .map_err(|error| format!("the table of mounts could not be read: {error}"))?;
    if !output.status.success() {
        return Err("the table of mounts could not be read: /sbin/mount failed".to_owned());
    }
    Ok(of_mount_output(&String::from_utf8_lossy(&output.stdout)))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn read_table() -> Result<Vec<(PathBuf, String)>, String> {
    Err("this system's table of mounts is not read by this build".to_owned())
}
