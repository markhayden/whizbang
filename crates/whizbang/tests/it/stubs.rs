//! `whizbang models`, `backends` and `setup`, as stubs (plan 1C, task C1a): each command
//! is there, says that it is not built, and exits as a complaint does, whatever it was
//! asked. What each will do arrives with its own task; none reads, writes, starts or
//! reaches anything until then.

use std::process::{Command, Output};

/// The three stubs, by the name each has on the command line.
const STUBS: [&str; 3] = ["models", "backends", "setup"];

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
fn each_stub_says_that_it_is_not_built_and_exits_1() {
    for name in STUBS {
        let out = whizbang(&[name]);
        // A complaint: exit 1, as every command's complaint is, and not the parser's 2.
        assert_eq!(out.status.code(), Some(1), "{name}");
        assert_eq!(text(&out.stdout), "", "{name}");
        let said = text(&out.stderr);
        assert!(
            said.starts_with(&format!("whizbang: {name} is not built in this version: ")),
            "{said}"
        );
        // One line, through the one place that prints a complaint.
        assert_eq!(said.matches('\n').count(), 1, "{said}");
        assert!(said.ends_with('\n'), "{said}");
    }
}

#[test]
fn a_stub_asked_for_anything_says_the_same_and_does_not_appear_to_work() {
    // The verbs of `SPEC.md` section 11, a model's name, an option nobody defined: a
    // stub does not take one for a mistake of the caller's, and does not act on one.
    // It says what it says when it is asked for nothing.
    let cases: [&[&str]; 8] = [
        &["models", "list"],
        &["models", "get", "sam3"],
        &["models", "remove", "sam3"],
        &["backends", "status"],
        &["backends", "status", "--probe"],
        &["backends", "--probe"],
        &["setup", "--offline"],
        &["setup", "list", "--help"],
    ];
    for args in cases {
        let bare = whizbang(&args[..1]);
        let out = whizbang(args);
        assert_eq!(out.status.code(), Some(1), "{args:?}");
        assert_eq!(text(&out.stdout), "", "{args:?}");
        // Byte for byte what it says when asked for nothing: so nothing it was given
        // is in it.
        assert_eq!(text(&out.stderr), text(&bare.stderr), "{args:?}");
    }
}

#[test]
fn the_help_lists_each_stub_and_says_that_it_is_not_built() {
    let out = whizbang(&["--help"]);
    assert_eq!(out.status.code(), Some(0));
    let top = text(&out.stdout);
    for name in STUBS {
        let line = top
            .lines()
            .find(|line| line.trim_start().starts_with(&format!("{name} ")))
            .unwrap_or_else(|| panic!("no line for {name} in:\n{top}"));
        assert!(line.contains("Not built"), "{line}");

        let out = whizbang(&[name, "--help"]);
        assert_eq!(out.status.code(), Some(0), "{name}");
        let help = text(&out.stdout);
        assert!(help.contains("Not built"), "{help}");
        assert!(help.contains("exits 1"), "{help}");
        // It offers no verb and no option of its own: only the parser's help.
        assert!(
            help.contains(&format!("\nUsage: whizbang {name}\n")),
            "{help}"
        );
        assert!(!help.contains("Commands:"), "{help}");
        assert!(!help.contains("Arguments:"), "{help}");
        assert_eq!(text(&out.stderr), "", "{name}");
    }
}
