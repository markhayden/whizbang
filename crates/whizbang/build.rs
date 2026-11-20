//! Tells the binary the commit it is built from (plan m4, task R1, item 2): `whizbang
//! --version` prints it beside the version, so that an installed binary can be held to
//! the commit a release was checked on.
//!
//! The commit is the one checked out when the binary was built, as `git rev-parse HEAD`
//! gives it in this crate's folder: 40 hexadecimal digits, or 64 in a repository of
//! SHA-256. It says nothing of files changed since that commit; whoever packages a
//! release holds the checkout to be clean (`scripts/package.sh` does not, and says so).
//! Built where there is no repository or no `git`, or inside a repository that does not
//! track this crate's manifest (a copy unpacked in somebody else's), the binary says
//! `unknown`, and `scripts/package.sh` refuses to make an archive of it.
//!
//! What a commit costs: this crate's targets are compiled again, which took 2 to 3
//! seconds for the binary, the library and the tests together when it was measured
//! (2026-11-19, a development build, which is incremental), so the commit is not kept in
//! a crate of its own.
//!
//! It is run again when the checked-out commit moves: when `HEAD` names another branch
//! or commit, when its log gains a line (every commit, checkout, merge and reset writes
//! one), and when the branch's own file or the packed refs change. Only files that exist
//! are named: cargo runs a build script again on every build when a file it was told to
//! watch is missing.

use std::path::Path;
use std::process::Command;

/// What `git` prints for these arguments, in this crate's folder: one line, trimmed. None
/// when `git` cannot be started, fails or prints nothing.
fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// Whether a text is a commit's whole name: 40 or 64 lower-case hexadecimal digits.
fn is_commit(text: &str) -> bool {
    matches!(text.len(), 40 | 64)
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn main() {
    // The repository git finds from here must be this crate's own, which holds this
    // crate's manifest as a tracked file: built from an unpacked copy inside somebody
    // else's repository, the commit found would be theirs.
    let own = git(&["ls-files", "--error-unmatch", "Cargo.toml"]).is_some();
    let commit = git(&["rev-parse", "--verify", "HEAD^{commit}"])
        .filter(|text| own && is_commit(text))
        .unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=WHIZBANG_COMMIT={commit}");
    println!("cargo:rerun-if-changed=build.rs");

    let mut watched = vec![
        "HEAD".to_owned(),
        "logs/HEAD".to_owned(),
        "packed-refs".to_owned(),
    ];
    // The branch `HEAD` names, if it names one: a commit on it moves its file and not `HEAD`.
    watched.extend(git(&["symbolic-ref", "-q", "HEAD"]));
    for name in watched {
        if let Some(path) = git(&["rev-parse", "--git-path", &name])
            && Path::new(&path).exists()
        {
            println!("cargo:rerun-if-changed={path}");
        }
    }
}
