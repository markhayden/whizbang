//! `whizbang spend status` and `reset`, through the binary (plan 1B, task S9): the
//! command line, the exit codes, and one round of a reset and a status over a store
//! on disk. What the commands say of each state of the books is held where they are
//! called in process (`spending.rs`, among the binary's own tests).

use std::process::Output;
use std::sync::Arc;

use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;
use whizbang_testkit::scratch::Scratch;

fn whizbang(args: &[&str]) -> Output {
    whizbang_testkit::binary::command(env!("CARGO_BIN_EXE_whizbang"))
        .args(args)
        .output()
        .expect("the binary runs")
}

fn text(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).expect("output is UTF-8")
}

#[test]
fn the_help_lists_spend_with_status_and_reset_and_says_what_a_reset_does() {
    let out = whizbang(&["--help"]);
    assert_eq!(out.status.code(), Some(0));
    let help = text(&out.stdout);
    let line = help
        .lines()
        .find(|line| line.trim_start().starts_with("spend "))
        .unwrap_or_else(|| panic!("no line for spend in:\n{help}"));
    assert!(line.contains("allowance"), "{line}");

    let out = whizbang(&["spend", "--help"]);
    assert_eq!(out.status.code(), Some(0));
    let help = text(&out.stdout);
    assert!(help.contains("status") && help.contains("reset"), "{help}");
    assert!(!help.contains("Not built"), "{help}");

    let out = whizbang(&["spend", "reset", "--help"]);
    assert_eq!(out.status.code(), Some(0));
    let help = text(&out.stdout);
    assert!(help.contains("--allowance <USD>"), "{help}");
    assert!(help.contains("--store <DIR>"), "{help}");
    assert!(help.contains("close the books"), "{help}");
    assert!(help.contains("is not stopped"), "{help}");

    let out = whizbang(&["spend", "status", "--help"]);
    let help = text(&out.stdout);
    assert!(help.contains("--run <RUN>"), "{help}");
    assert!(help.contains("Reads only"), "{help}");
    // A reservation shown as open may be a dead process's, and a reset clears it.
    let flat = help.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        flat.contains(
            "shown as \"open\" belongs to a run that is still executing, or to a process \
             that has died"
        ),
        "{help}"
    );
    assert!(flat.contains("`spend reset` clears it"), "{help}");
}

#[test]
fn spend_without_a_command_or_a_store_is_the_parsers_usage_error() {
    for args in [
        &["spend"][..],
        &["spend", "status"],
        &["spend", "reset", "--allowance", "1"],
        &["spend", "reset", "--store", "somewhere"],
        &["spend", "adjust"],
    ] {
        let out = whizbang(args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert_eq!(text(&out.stdout), "", "{args:?}");
    }
}

#[test]
fn a_reset_and_a_status_through_the_binary_and_a_complaint_that_is_escaped() {
    let dir = Scratch::new("spend-binary");
    let root = dir.path().join("store");
    let store = root.to_str().unwrap();
    // No store there: a complaint, exit 1, and nothing is made.
    for args in [
        &["spend", "status", "--store", store][..],
        &["spend", "reset", "--allowance", "5", "--store", store],
    ] {
        let out = whizbang(args);
        assert_eq!(out.status.code(), Some(1), "{args:?}");
        assert_eq!(text(&out.stdout), "");
        let said = text(&out.stderr);
        assert!(
            said.starts_with("whizbang: there is no store at "),
            "{said}"
        );
    }
    assert!(!root.exists());
    drop(Store::open(&root, Arc::new(RealFs)).expect("a store"));

    let out = whizbang(&["spend", "reset", "--allowance", "5", "--store", store]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(text(&out.stdout).starts_with("allowance reset: $5.00\nepoch: 1\n"));
    let out = whizbang(&["spend", "status", "--store", store]);
    assert_eq!(out.status.code(), Some(0));
    let said = text(&out.stdout);
    assert!(
        said.starts_with("epoch: 1\nallowance: $5.00\ncounted against it: $0.00\n"),
        "{said}"
    );
    assert!(said.contains("left: $5.00\n"), "{said}");
    let out = whizbang(&["spend", "status", "--store", store, "--run", "7000-1"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(text(&out.stdout).starts_with("run: 7000-1\n"));

    // An allowance that is none is a complaint, and what was typed is escaped.
    let out = whizbang(&[
        "spend",
        "reset",
        "--allowance",
        "lots\u{1b}[31m",
        "--store",
        store,
    ]);
    assert_eq!(out.status.code(), Some(1));
    let said = text(&out.stderr);
    assert!(
        said.starts_with("whizbang: lots\\u{1b}[31m is no allowance: "),
        "{said}"
    );
    assert!(!said.contains('\u{1b}'), "{said}");
}
