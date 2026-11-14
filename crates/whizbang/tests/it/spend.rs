//! `whizbang spend`, as a stub (plan 1B, task B0b): the command is there, says that it
//! is not built, and exits as a complaint does. The ledger behind it, and `status` and
//! `reset`, arrive with their own tasks.

use std::process::{Command, Output};

fn whizbang(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_whizbang"))
        .args(args)
        .output()
        .expect("the binary runs")
}

fn text(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).expect("output is UTF-8")
}

#[test]
fn spend_says_that_it_is_not_built_and_exits_1() {
    let out = whizbang(&["spend"]);
    // A complaint: exit 1, as every command's complaint is, and not the parser's 2.
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(text(&out.stdout), "");
    let said = text(&out.stderr);
    assert!(said.starts_with("whizbang: "), "{said}");
    assert!(
        said.contains("spend") && said.contains("not built"),
        "{said}"
    );
    // One line, through the one place that prints a complaint.
    assert_eq!(said.matches('\n').count(), 1, "{said}");
    assert!(said.ends_with('\n'), "{said}");
}

#[test]
fn the_help_lists_spend_and_says_that_it_is_not_built() {
    let out = whizbang(&["--help"]);
    assert_eq!(out.status.code(), Some(0));
    let help = text(&out.stdout);
    let line = help
        .lines()
        .find(|line| line.trim_start().starts_with("spend "))
        .unwrap_or_else(|| panic!("no line for spend in:\n{help}"));
    assert!(line.contains("Not built"), "{line}");

    let out = whizbang(&["spend", "--help"]);
    assert_eq!(out.status.code(), Some(0));
    let help = text(&out.stdout);
    assert!(help.contains("Not built"), "{help}");
    assert!(help.contains("exits 1"), "{help}");
    // Where its commands are: not here yet.
    assert!(help.contains("status") && help.contains("reset"), "{help}");
    assert!(help.contains("arrive with later tasks"), "{help}");
    assert_eq!(text(&out.stderr), "");
}

#[test]
fn spend_takes_no_argument_yet_and_an_argument_is_the_parsers_usage_error() {
    // `status` and `reset` are not commands of this build: the parser says so, with its
    // own exit code, and nothing is read or written.
    for word in ["status", "reset"] {
        let out = whizbang(&["spend", word]);
        assert_eq!(out.status.code(), Some(2), "{word}");
        assert_eq!(text(&out.stdout), "", "{word}");
    }
}
