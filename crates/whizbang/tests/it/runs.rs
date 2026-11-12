//! `echo` run through the executor the binary builds, and `whizbang runs list|show` on
//! what it left: the real binary, on a real store.

use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use whizbang_core::contract::{Call, Executor, Outcome, RUN_RECORD_SCHEMA, RunId, Value};
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Kind, Store};
use whizbang_testkit::echo::Echo;
use whizbang_testkit::schema::validate;
use whizbang_testkit::scratch::Scratch;

struct Ran {
    code: Option<i32>,
    out: Vec<u8>,
    err: String,
}

impl Ran {
    fn text(&self) -> &str {
        std::str::from_utf8(&self.out).expect("output is UTF-8")
    }
}

fn whizbang(args: &[&str], root: &Path) -> Ran {
    let out = Command::new(env!("CARGO_BIN_EXE_whizbang"))
        .arg("runs")
        .args(args)
        .arg("--store")
        .arg(root)
        .output()
        .expect("the binary runs");
    Ran {
        code: out.status.code(),
        out: out.stdout,
        err: String::from_utf8(out.stderr).expect("output is UTF-8"),
    }
}

fn echo(text: &str) -> Call {
    Call::new("echo").argument("text", Value::text(text))
}

/// A store in which the binary's own executor, with `echo` registered by the test, ran
/// one `echo` that completed and one that failed. The ids of the two runs.
fn with_two_runs(root: &Path) -> (String, String) {
    let store = Arc::new(Store::open(root, Arc::new(RealFs)).unwrap());
    let mut registry = whizbang::wiring::registry().expect("the registry builds");
    registry.register(Arc::new(Echo::new())).unwrap();
    let executor = whizbang::wiring::runner(registry, store);
    let completed = executor.submit(&echo("hello")).unwrap();
    assert_eq!(completed.outcome().name(), "completed");
    let failed = executor
        .submit(&echo("hello again").argument("fail", Value::text("it broke")))
        .unwrap();
    assert!(matches!(failed.outcome(), Outcome::Error { .. }));
    (
        completed.run().as_str().to_owned(),
        failed.run().as_str().to_owned(),
    )
}

#[test]
fn echo_runs_through_the_binarys_executor_and_runs_list_and_show_read_it() {
    let dir = Scratch::new("runs");
    let root = dir.path().join("store");
    let (completed, failed) = with_two_runs(&root);
    RunId::parse(&completed).unwrap();

    let listed = whizbang(&["list"], &root);
    assert_eq!(listed.code, Some(0), "{}", listed.err);
    let lines: Vec<&str> = listed.text().lines().collect();
    assert_eq!(lines[0], "runs: 2");
    assert_eq!(lines.len(), 3);
    let fields: Vec<&str> = lines[1].split('\t').collect();
    assert_eq!(
        &fields[..3],
        [completed.as_str(), "completed", "echo 1.0.0"]
    );
    assert!(
        fields[3].ends_with('Z') && fields[3].contains('T'),
        "{}",
        fields[3]
    );
    assert!(fields[4].ends_with(" ms"));
    let fields: Vec<&str> = lines[2].split('\t').collect();
    assert_eq!(&fields[..3], [failed.as_str(), "error", "echo 1.0.0"]);

    // The record as it is stored validates against the record's schema.
    let shown = whizbang(&["show", &completed, "--json"], &root);
    assert_eq!(shown.code, Some(0), "{}", shown.err);
    assert!(shown.text().ends_with("}\n") && shown.text().lines().count() == 1);
    let document: serde_json::Value = serde_json::from_slice(&shown.out).unwrap();
    let schema: serde_json::Value = serde_json::from_str(RUN_RECORD_SCHEMA).unwrap();
    validate(&schema, &document).unwrap();
    assert_eq!(document["identity"]["run"], completed.as_str());
    assert_eq!(document["identity"]["whizbang"], env!("CARGO_PKG_VERSION"));
    assert_eq!(document["outcome"]["status"], "completed");

    // For a person: every group, the empty ones said to be empty.
    let shown = whizbang(&["show", &completed], &root);
    assert_eq!(shown.code, Some(0), "{}", shown.err);
    let text = shown.text();
    for line in [
        format!("run: {completed}"),
        "request: none".to_owned(),
        "parent: none".to_owned(),
        "action: echo 1.0.0".to_owned(),
        "status: completed".to_owned(),
        "settings: none".to_owned(),
        "arguments: 1".to_owned(),
        "  text = \"hello\"".to_owned(),
        "inputs: none".to_owned(),
        "outputs: none".to_owned(),
        "models: none".to_owned(),
        "spend: none".to_owned(),
        "egress: none".to_owned(),
        "steps: none".to_owned(),
        "judge: none".to_owned(),
        "diagnostics: 2".to_owned(),
    ] {
        assert!(
            text.lines().any(|l| l == line),
            "{line:?} is not in:\n{text}"
        );
    }
    assert!(text.contains("config: sha256:e3b0c442"));
    assert!(text.contains("  journal\t"));
}

#[test]
fn a_failed_runs_record_and_diagnostics_are_kept_and_readable() {
    let dir = Scratch::new("runs");
    let root = dir.path().join("store");
    let (_, failed) = with_two_runs(&root);
    let shown = whizbang(&["show", &failed], &root);
    assert_eq!(shown.code, Some(0), "{}", shown.err);
    let text = shown.text();
    assert!(text.contains("status: error\n"), "{text}");
    assert!(text.contains("error: echo: it broke\n"), "{text}");
    assert!(text.contains("outputs: none\n"), "{text}");
    assert!(!text.contains("completed"), "{text}");
    // What it reported and wrote before it failed.
    let journal = whizbang(&["show", &failed, "--diagnostic", "journal"], &root);
    assert_eq!(journal.code, Some(0), "{}", journal.err);
    assert_eq!(journal.text().lines().count(), 1);
    assert!(journal.text().contains(r#""message":"echo: hello again""#));
    let written = whizbang(
        &["show", &failed, "--diagnostic", "diagnostics/echo.txt"],
        &root,
    );
    assert_eq!(written.out, b"hello again");
    // The record itself is one of the run's names.
    let record = whizbang(&["show", &failed, "--diagnostic", "record"], &root);
    let json = whizbang(&["show", &failed, "--json"], &root);
    assert_eq!(record.out, json.out);
    // `store list` counts what the run holds: no output, three diagnostics.
    let out = Command::new(env!("CARGO_BIN_EXE_whizbang"))
        .args(["store", "list", "--store"])
        .arg(&root)
        .output()
        .unwrap();
    let listed = String::from_utf8(out.stdout).unwrap();
    assert!(
        listed.contains(&format!("{failed}\toutputs 0\tdiagnostics 3\t")),
        "{listed}"
    );
}

#[test]
fn what_cannot_be_shown_is_a_complaint_and_not_a_crash() {
    let dir = Scratch::new("runs");
    let root = dir.path().join("store");
    let (completed, _) = with_two_runs(&root);
    let complaint = |args: &[&str], what: &str| {
        let ran = whizbang(args, &root);
        assert_eq!(ran.code, Some(1), "{args:?}: {}", ran.text());
        assert!(ran.out.is_empty());
        assert!(ran.err.contains(what), "{args:?}: {}", ran.err);
    };
    complaint(&["show", "absent"], "the store holds no run absent");
    complaint(
        &["show", &completed, "--diagnostic", "nothing"],
        "holds nothing named nothing",
    );
    // What is echoed of an argument is escaped and cut.
    let long = format!("a\u{1b}[31m{}", "z".repeat(500));
    let ran = whizbang(&["show", &long], &root);
    assert_eq!(ran.code, Some(1));
    assert!(!ran.err.contains('\u{1b}'), "{:?}", ran.err);
    assert!(ran.err.len() < 200, "{}", ran.err);
    // No store: neither command makes one.
    let nowhere = dir.path().join("nowhere");
    let ran = whizbang(&["list"], &nowhere);
    assert_eq!(ran.code, Some(1));
    assert!(ran.err.contains("there is no store"), "{}", ran.err);
    assert!(!nowhere.exists());
    // --json and --diagnostic are two questions.
    let ran = whizbang(
        &["show", &completed, "--json", "--diagnostic", "journal"],
        &root,
    );
    assert_eq!(ran.code, Some(2), "{}", ran.err);
}

#[test]
fn runs_that_other_hands_made_are_listed_and_shown_as_they_are() {
    // A store is not trusted to hold only what this build wrote. A run with an id the
    // executor would never make, with a control character in it; a run with no record;
    // a run whose record is not one; a run holding an output.
    let dir = Scratch::new("runs");
    let root = dir.path().join("store");
    let (completed, _) = with_two_runs(&root);
    let json = whizbang(&["show", &completed, "--json"], &root);
    let loose = "Loose \u{1b}[2J Run";
    assert!(RunId::parse(loose).is_err());
    let renamed = json.text().replace(
        &format!(r#""run":"{completed}""#),
        r#""run":"Loose \u001b[2J Run""#,
    );
    {
        let store = Store::open_existing(&root, Arc::new(RealFs)).unwrap();
        let mut publication = store.begin().unwrap();
        let mut put = |run: &str, name: &str, kind: Kind, bytes: &[u8]| {
            publication.put_in(run, name, kind, bytes).unwrap();
        };
        put("Loose Run", "record", Kind::Diagnostic, renamed.as_bytes());
        put("bare", "mask", Kind::Output, b"an output");
        put(
            "broken",
            "record",
            Kind::Diagnostic,
            b"\x1b[31mnot a record\n",
        );
        put("stolen", "record", Kind::Diagnostic, &json.out);
        let published = publication.publish().unwrap();
        // The store refuses an id with a control character; a generation file that
        // other hands wrote can still hold one. This one is edited to.
        let file = root
            .join("generations")
            .join(format!("{}.json", published.generation));
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains(r#""Loose Run""#));
        let text = text.replace(r#""Loose Run""#, r#""Loose \u001b[2J Run""#);
        std::fs::remove_file(&file).unwrap();
        std::fs::write(&file, text).unwrap();
    }

    let listed = whizbang(&["list"], &root);
    assert_eq!(listed.code, Some(0), "{}", listed.err);
    let text = listed.text();
    assert!(!text.contains('\u{1b}'), "{text:?}");
    assert!(text.starts_with("runs: 6\n"), "{text}");
    assert!(
        text.contains("Loose \\u{1b}[2J Run\tcompleted\techo 1.0.0\t"),
        "{text}"
    );
    assert!(text.contains("bare\tno record\n"), "{text}");
    assert!(
        text.contains("broken\trecord not read: not a run record"),
        "{text}"
    );
    assert!(
        text.contains("stolen\trecord not read: the record names another run"),
        "{text}"
    );

    // A run that `list` shows can be shown, whatever its id.
    let shown = whizbang(&["show", loose], &root);
    assert_eq!(shown.code, Some(0), "{}", shown.err);
    assert!(!shown.text().contains('\u{1b}'));
    assert!(shown.text().starts_with("run: Loose \\u{1b}[2J Run\n"));
    let shown = whizbang(&["show", "bare"], &root);
    assert_eq!(shown.code, Some(1));
    assert!(shown.err.contains("the run has no record"), "{}", shown.err);
    let shown = whizbang(&["show", "broken"], &root);
    assert_eq!(shown.code, Some(1));
    assert!(!shown.err.contains('\u{1b}'), "{:?}", shown.err);
    // An output is not handed out as a diagnostic.
    let shown = whizbang(&["show", "bare", "--diagnostic", "mask"], &root);
    assert_eq!(shown.code, Some(1));
    assert!(
        shown.err.contains("is an output, not a diagnostic"),
        "{}",
        shown.err
    );
}
