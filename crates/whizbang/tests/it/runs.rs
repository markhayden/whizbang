//! `echo` run through the executor the binary builds, and `whizbang runs list|show` on
//! what it left: the real binary, on a real store.

use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use whizbang_core::artifact::{ArtifactType, Location, Origin, Shape, Step};
use whizbang_core::contract::OutputPort;
use whizbang_core::contract::{
    Action, ActionDescriptor, Admission, Answer, Call, CallError, Cancel, Context, Executor,
    Failure, Outcome, PreparedCall, Produced, RUN_RECORD_SCHEMA, Request, RequestId, RunId,
    RunState, Value,
};
use whizbang_core::identity::{Binding, Digest, PixelDigest, Space};
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Kind, Store};
use whizbang_testkit::echo::Echo;
use whizbang_testkit::fs_shim::ShimFs;
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
        "refusals: none".to_owned(),
        "settings: none".to_owned(),
        "arguments: 1".to_owned(),
        "  text = \"hello\"".to_owned(),
        "inputs: none".to_owned(),
        "outputs: none".to_owned(),
        "models: none".to_owned(),
        "spend: nothing reserved; paid requests: none".to_owned(),
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
fn a_cancelled_run_is_listed_and_shown_as_cancelled_with_its_diagnostics() {
    // `echo`, cancelled the moment its run is admitted, through the binary's executor:
    // the hook is the file system's, straight after the admission becomes visible.
    let dir = Scratch::new("runs");
    let root = dir.path().join("store");
    let fs = Arc::new(ShimFs::new());
    let store = Arc::new(Store::open(&root, fs.clone()).unwrap());
    let mut registry = whizbang::wiring::registry().expect("the registry builds");
    registry.register(Arc::new(Echo::new())).unwrap();
    let executor = whizbang::wiring::runner(registry, store.clone());
    fs.pause_after_rename_onto("CURRENT", {
        let (executor, store) = (executor.clone(), store.clone());
        move || {
            let generation = store.snapshot().unwrap();
            let run = RunId::parse(generation.runs().keys().next().unwrap()).unwrap();
            assert_eq!(executor.cancel(&run), Ok(Cancel::Accepted));
        }
    });
    let request = Request::new(echo("hello")).with_id(RequestId::parse("order-9").unwrap());
    let Ok(Answer::Ran(finished)) = executor.request(&request) else {
        panic!("the request did not run");
    };
    assert_eq!(finished.outcome(), &Outcome::Cancelled);
    let run = finished.run().as_str();
    drop(executor);
    drop(store);

    let listed = whizbang(&["list"], &root);
    assert_eq!(listed.code, Some(0), "{}", listed.err);
    let lines: Vec<&str> = listed.text().lines().collect();
    assert_eq!(lines[0], "runs: 1");
    let fields: Vec<&str> = lines[1].split('\t').collect();
    assert_eq!(&fields[..3], [run, "cancelled", "echo 1.0.0"]);

    let shown = whizbang(&["show", run], &root);
    assert_eq!(shown.code, Some(0), "{}", shown.err);
    let text = shown.text();
    for line in [
        format!("run: {run}"),
        "request: order-9".to_owned(),
        "status: cancelled".to_owned(),
        "outputs: none".to_owned(),
        "diagnostics: 1".to_owned(),
    ] {
        assert!(
            text.lines().any(|l| l == line),
            "{line:?} is not in:\n{text}"
        );
    }
    let shown = whizbang(&["show", run, "--json"], &root);
    let document: serde_json::Value = serde_json::from_slice(&shown.out).unwrap();
    let schema: serde_json::Value = serde_json::from_str(RUN_RECORD_SCHEMA).unwrap();
    validate(&schema, &document).unwrap();
    assert_eq!(document["outcome"]["status"], "cancelled");
    assert_eq!(document["outcome"]["error"], serde_json::Value::Null);
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
    // `store list` counts what the run holds: no output, and four diagnostic
    // references: the admission, what the action wrote, the journal and the record.
    let out = Command::new(env!("CARGO_BIN_EXE_whizbang"))
        .args(["store", "list", "--store"])
        .arg(&root)
        .output()
        .unwrap();
    let listed = String::from_utf8(out.stdout).unwrap();
    assert!(
        listed.contains(&format!("{failed}\toutputs 0\tdiagnostics 4\t")),
        "{listed}"
    );
}

/// An action that returns one location.
struct Find(ActionDescriptor);

fn the_place() -> Location {
    let photo = Binding::root(
        PixelDigest(Digest::of_bytes(b"a photo")),
        Space::flat(8, 6).unwrap(),
    );
    let origin = Origin::new(Step::new("find", "1.0.0")).unwrap();
    Location::new(photo, Shape::WholeImage, origin).unwrap()
}

impl Action for Find {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }

    fn run(&self, _: &PreparedCall, _: &Context) -> Result<Produced, Failure> {
        Ok(Produced::nothing().with("where", the_place()))
    }
}

#[test]
fn a_run_that_committed_an_output_is_shown_and_listed_with_it() {
    let dir = Scratch::new("runs");
    let root = dir.path().join("store");
    let run = {
        let store = Arc::new(Store::open(&root, Arc::new(RealFs)).unwrap());
        let mut registry = whizbang::wiring::registry().expect("the registry builds");
        let find = whizbang_core::contract::Kind::Primitive;
        let find = ActionDescriptor::draft("find", "1.0.0", find, "A test action.").output(
            OutputPort::one("where", ArtifactType::Location, "Where it is."),
        );
        registry
            .register(Arc::new(Find(find.build().unwrap())))
            .unwrap();
        let executor = whizbang::wiring::runner(registry, store);
        let finished = executor.submit(&Call::new("find")).unwrap();
        assert_eq!(finished.outcome().outputs().len(), 1);
        finished.run().as_str().to_owned()
    };
    let shown = whizbang(&["show", &run], &root);
    assert_eq!(shown.code, Some(0), "{}", shown.err);
    let id = the_place().id();
    assert!(
        shown
            .text()
            .contains(&format!("outputs: 1\n  where\tlocation {id}\n")),
        "{}",
        shown.text()
    );
    // `store list` counts what the run holds: one output, and its admission, its
    // journal and its record.
    let out = Command::new(env!("CARGO_BIN_EXE_whizbang"))
        .args(["store", "list", "--store"])
        .arg(&root)
        .output()
        .unwrap();
    let listed = String::from_utf8(out.stdout).unwrap();
    assert!(
        listed.contains(&format!("{run}\toutputs 1\tdiagnostics 3\t")),
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

#[test]
fn every_listed_run_can_be_shown_from_its_listed_line() {
    // `list` escapes an id that holds a control character and cuts one over 64
    // characters, so what it prints is not always the id. `show` takes the id as it is
    // listed too: copied from a line of `list`, it finds the run.
    let dir = Scratch::new("runs");
    let root = dir.path().join("store");
    let (completed, _) = with_two_runs(&root);
    let json = whizbang(&["show", &completed, "--json"], &root);
    let record_of = |run: &str| {
        let named = format!(r#""run":{}"#, serde_json::to_string(run).unwrap());
        let record = json
            .text()
            .replace(&format!(r#""run":"{completed}""#), &named);
        assert!(record.contains(&named));
        record
    };
    let long = format!("long-{}", "z".repeat(70));
    let twins = [
        format!("{}-one", "t".repeat(64)),
        format!("{}-two", "t".repeat(64)),
    ];
    // An id that is, letter for letter, how another id is listed.
    let escaped = "Loose \\u{1b}[2J Run";
    {
        let store = Store::open_existing(&root, Arc::new(RealFs)).unwrap();
        let mut publication = store.begin().unwrap();
        let mut ids = vec!["Loose Run", long.as_str(), escaped];
        ids.extend(twins.iter().map(String::as_str));
        for id in ids {
            let record = record_of(&id.replace("Loose Run", "Loose \u{1b}[2J Run"));
            let kind = Kind::Diagnostic;
            publication
                .put_in(id, "record", kind, record.as_bytes())
                .unwrap();
        }
        let published = publication.publish().unwrap();
        let file = root
            .join("generations")
            .join(format!("{}.json", published.generation));
        let text = std::fs::read_to_string(&file).unwrap();
        let text = text.replace(r#""Loose Run""#, r#""Loose \u001b[2J Run""#);
        std::fs::remove_file(&file).unwrap();
        std::fs::write(&file, text).unwrap();
    }
    let listed = whizbang(&["list"], &root);
    assert_eq!(listed.code, Some(0), "{}", listed.err);
    let lines: Vec<&str> = listed.text().lines().skip(1).collect();
    assert_eq!(lines.len(), 7, "{}", listed.text());
    let mut twice = 0;
    for line in lines {
        let (id, rest) = line.split_once('\t').unwrap();
        assert!(!rest.is_empty(), "{line}");
        let shown = whizbang(&["show", id], &root);
        if id.starts_with("tttt") {
            // Two runs are listed so: `show` says that, and shows neither.
            twice += 1;
            assert_eq!(shown.code, Some(1), "{id}");
            assert!(
                shown.err.contains("2 runs are listed as tttt")
                    && shown.err.contains("`whizbang store list` prints"),
                "{}",
                shown.err
            );
            continue;
        }
        if id == escaped {
            // This line is of the run whose id holds the escape character, and its
            // text is also, letter for letter, the id of another run: `show` refuses
            // to choose, and the last part of this test says what it says.
            assert_eq!(shown.code, Some(1), "{id}");
            continue;
        }
        assert_eq!(shown.code, Some(0), "{id}: {}", shown.err);
        // The run that was shown is the one of that line.
        let first = shown.text().lines().next().unwrap();
        assert_eq!(first, format!("run: {id}"), "{id}");
    }
    assert_eq!(twice, 2);
    // A text that is the id of one run and how another run is listed is not taken
    // for either: an id always means its own run, and here that would show a run
    // other than the one whose line was copied. Both are named, escaped. The run
    // with the escape character is shown by its id itself, and the other by what
    // `list` prints of it, which is no run's id.
    let both = whizbang(&["show", escaped, "--json"], &root);
    assert_eq!(both.code, Some(1));
    assert!(both.out.is_empty());
    assert_eq!(
        both.err,
        "whizbang: Loose \\u{1b}[2J Run is the id of one run and is how `runs list` prints \
         another run. `runs list` prints the first as Loose \\\\u{1b}[2J Run: give that for \
         it. Give the second by its id, which holds a character that is printed escaped, or \
         is longer than is printed; `whizbang store list` prints ids whole, with their \
         escapes\n"
    );
    let raw = whizbang(&["show", "Loose \u{1b}[2J Run", "--json"], &root);
    assert!(
        raw.text().contains(r#""run":"Loose \u001b[2J Run""#),
        "{}",
        raw.text()
    );
    let spelt_so = whizbang(&["show", "Loose \\\\u{1b}[2J Run", "--json"], &root);
    assert!(
        spelt_so.text().contains(r#""run":"Loose \\u{1b}[2J Run""#),
        "{}",
        spelt_so.text()
    );
    // An id comes before what is listed: with the run of the escape character gone,
    // the text means the run of that id and no other.
    {
        let store = Store::open_existing(&root, Arc::new(RealFs)).unwrap();
        store.delete(&["Loose \u{1b}[2J Run"]).unwrap();
    }
    let exact = whizbang(&["show", escaped, "--json"], &root);
    assert!(
        exact.text().contains(r#""run":"Loose \\u{1b}[2J Run""#),
        "{}",
        exact.err
    );
    // An id that `list` cut is taken whole as well.
    for twin in &twins {
        assert_eq!(whizbang(&["show", twin], &root).code, Some(0));
    }
    // The cut id of the long run, as listed, finds it.
    let cut: String = long.chars().take(64).collect();
    let found = whizbang(&["show", &format!("{cut}..."), "--json"], &root);
    assert!(found.text().contains(&long), "{}", found.err);
    // What is listed as no run is still no run.
    let ran = whizbang(&["show", "absent..."], &root);
    assert_eq!(ran.code, Some(1));
    assert!(
        ran.err.contains("the store holds no run absent..."),
        "{}",
        ran.err
    );
}

#[test]
fn a_refusal_is_printed_as_the_core_says_it_once_escaped_and_whole() {
    // A record that is not one, with an escape sequence in a field's name. The core's
    // sentence quotes it escaped; the command prints that sentence as it is, so a
    // backslash is one backslash and the sentence is not cut a second time.
    let dir = Scratch::new("runs");
    let root = dir.path().join("store");
    with_two_runs(&root);
    {
        let store = Store::open_existing(&root, Arc::new(RealFs)).unwrap();
        let mut publication = store.begin().unwrap();
        publication
            .put_in(
                "broken",
                "record",
                Kind::Diagnostic,
                b"{\"record\":\"run/2\",\"\\u001b[2J\":1}\n",
            )
            .unwrap();
        publication.publish().unwrap();
    }
    let expected = "not a run record: unknown field `\\u{1b}[2J`, expected one of `record`";
    let listed = whizbang(&["list"], &root);
    let line = listed
        .text()
        .lines()
        .find(|line| line.starts_with("broken\t"))
        .unwrap();
    assert!(line.contains(expected), "{line}");
    assert!(line.contains(" at line 1 column "), "{line}");
    let shown = whizbang(&["show", "broken"], &root);
    assert_eq!(shown.code, Some(1));
    assert!(shown.err.contains(expected), "{}", shown.err);
    assert!(shown.err.contains(" at line 1 column "), "{}", shown.err);
    assert!(!shown.err.contains('\u{1b}'));

    // A record of the version before this one is another version, and is said to be.
    {
        let store = Store::open_existing(&root, Arc::new(RealFs)).unwrap();
        let mut publication = store.begin().unwrap();
        publication
            .put_in(
                "older",
                "record",
                Kind::Diagnostic,
                b"{\"record\":\"run/1\",\"spend\":[]}\n",
            )
            .unwrap();
        publication.publish().unwrap();
    }
    let shown = whizbang(&["show", "older"], &root);
    assert_eq!(shown.code, Some(1));
    assert!(
        shown
            .err
            .contains("a record of another version: run/1, and this build reads run/2"),
        "{}",
        shown.err
    );
    // And `runs list` lists the run with the same reason, in place of its fields.
    let listed = whizbang(&["list"], &root);
    assert_eq!(listed.code, Some(0), "{}", listed.err);
    let expected = "older\trecord not read: a record of another version: run/1, and this build \
                    reads run/2";
    assert!(
        listed.text().lines().any(|line| line == expected),
        "{}",
        listed.text()
    );
}

#[test]
fn runs_show_escapes_every_text_a_run_was_given() {
    // An escape sequence in an argument's text, in an error's message and in the reason
    // a run was invalid: `show` prints none of it as a control character.
    let dir = Scratch::new("runs");
    let root = dir.path().join("store");
    let (failed, invalid) = {
        let store = Arc::new(Store::open(&root, Arc::new(RealFs)).unwrap());
        let mut registry = whizbang::wiring::registry().unwrap();
        registry.register(Arc::new(Echo::new())).unwrap();
        let executor = whizbang::wiring::runner(registry, store);
        let failed = executor
            .submit(
                &echo("text \u{1b}[31m red").argument("fail", Value::text("it \u{1b}[2J broke")),
            )
            .unwrap();
        let invalid = executor
            .submit(&echo("fine").argument("invalid", Value::text("no \u{7} bell")))
            .unwrap();
        (
            failed.run().as_str().to_owned(),
            invalid.run().as_str().to_owned(),
        )
    };
    let plain = |text: &str| {
        !text
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
    };
    let shown = whizbang(&["show", &failed], &root);
    assert_eq!(shown.code, Some(0), "{}", shown.err);
    let text = shown.text();
    assert!(plain(text), "{text:?}");
    assert!(
        text.contains("error: echo: it \\u{1b}[2J broke\n"),
        "{text}"
    );
    assert!(
        text.contains("  text = \"text \\u{1b}[31m red\"\n"),
        "{text}"
    );
    let shown = whizbang(&["show", &invalid], &root);
    let text = shown.text();
    assert!(plain(text), "{text:?}");
    assert!(text.contains("invalid: no \\u{7} bell\n"), "{text}");
}

#[test]
fn a_reader_that_stops_reading_is_not_an_error() {
    // The command's output goes to a pipe nobody reads, as `whizbang runs list | head -1`
    // leaves it once `head` has gone. That is not a failure: nothing is said, and the
    // exit is 0. The same for `store list` and `describe`.
    let dir = Scratch::new("runs");
    let root = dir.path().join("store");
    let (completed, _) = with_two_runs(&root);
    let store = root.to_str().unwrap();
    for args in [
        vec!["runs", "list", "--store", store],
        vec!["runs", "show", &completed, "--json", "--store", store],
        vec!["store", "list", "--store", store],
        vec!["describe"],
    ] {
        let (reader, writer) = std::io::pipe().unwrap();
        drop(reader);
        let out = Command::new(env!("CARGO_BIN_EXE_whizbang"))
            .args(&args)
            .stdout(writer)
            .output()
            .unwrap();
        let err = String::from_utf8(out.stderr).unwrap();
        assert_eq!(out.status.code(), Some(0), "{args:?}: {err}");
        assert!(err.is_empty(), "{args:?}: {err}");
    }
}

/// An action whose run cannot be ended: it has the store's file system refuse the next
/// call that changes the disk, which is the first of the publication that ends its run.
struct Boom(ActionDescriptor, Arc<ShimFs>);

impl Action for Boom {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }

    fn run(&self, _: &PreparedCall, _: &Context) -> Result<Produced, Failure> {
        self.1.refuse_before(self.1.events().len());
        Ok(Produced::nothing())
    }
}

#[test]
fn a_run_that_was_admitted_and_did_not_end_is_listed_and_shown_as_such() {
    // A run is in the store from its admission. One whose end the store could not
    // write never ends: it stays as it was admitted, which is also what a process that
    // died while an action ran leaves. `runs list` and `runs show` say so, with what
    // the admission holds.
    let dir = Scratch::new("runs");
    let root = dir.path().join("store");
    let id = RequestId::parse("order-17").unwrap();
    let request = |action: &str| Request::new(Call::new(action)).with_id(id.clone());
    {
        let fs = Arc::new(ShimFs::new());
        let store = Arc::new(Store::open(&root, fs.clone()).unwrap());
        let mut registry = whizbang::wiring::registry().expect("the registry builds");
        let boom = whizbang_core::contract::Kind::Primitive;
        let boom = ActionDescriptor::draft("boom", "1.2.3", boom, "A test action.");
        registry
            .register(Arc::new(Boom(boom.build().unwrap(), fs)))
            .unwrap();
        let executor = whizbang::wiring::runner(registry, store);
        let unended = executor.request(&request("boom"));
        assert!(matches!(unended, Err(CallError::Store(_))), "{unended:?}");
        // The same request again, in the process that admitted it, is not run again.
        let Ok(Answer::Repeat(earlier)) = executor.request(&request("boom")) else {
            panic!("the request ran again");
        };
        let abandoned = matches!(earlier.state(), RunState::Abandoned { failure: Some(_) });
        assert!(abandoned, "{:?}", earlier.state());
    }
    let listed = whizbang(&["list"], &root);
    assert_eq!(listed.code, Some(0), "{}", listed.err);
    let lines: Vec<&str> = listed.text().lines().collect();
    assert_eq!(lines[0], "runs: 1");
    let fields: Vec<&str> = lines[1].split('\t').collect();
    let run = fields[0].to_owned();
    assert_eq!(
        fields[1..3],
        ["admitted, not ended", "boom 1.2.3"],
        "{lines:?}"
    );
    assert!(fields[3].ends_with('Z'), "{lines:?}");
    assert_eq!(fields.len(), 4);

    let shown = whizbang(&["show", &run], &root);
    assert_eq!(shown.code, Some(0), "{}", shown.err);
    let text = shown.text();
    assert!(
        text.starts_with(&format!(
            "run: {run}\nrequest: order-17\nparent: none\naction: boom 1.2.3\nstatus: admitted, not ended\n\
             started: "
        )),
        "{text}"
    );
    assert!(
        text.ends_with(
            "spend: nothing reserved\nrecord: none: the run was admitted and has not ended; it is executing, or the \
             process that ran it ended first\n"
        ),
        "{text}"
    );
    assert_eq!(text.lines().count(), 8, "{text}");
    // Its admission is one of its names, and is printed as it is stored.
    let admission = whizbang(&["show", &run, "--diagnostic", "admitted"], &root);
    let admission = Admission::from_bytes(&admission.out).unwrap();
    assert_eq!(admission.run().as_str(), run);
    assert_eq!(admission.request(), Some(&id));
    // There is no record to print as JSON, and the complaint says why.
    let json = whizbang(&["show", &run, "--json"], &root);
    assert_eq!(json.code, Some(1), "{}", json.err);
    assert!(
        json.err
            .contains("has no record: it was admitted and has not ended"),
        "{}",
        json.err
    );
    assert!(json.out.is_empty());
}

#[test]
fn a_run_shows_the_request_id_it_was_made_for() {
    let dir = Scratch::new("runs");
    let root = dir.path().join("store");
    let run = {
        let store = Arc::new(Store::open(&root, Arc::new(RealFs)).unwrap());
        let mut registry = whizbang::wiring::registry().expect("the registry builds");
        registry.register(Arc::new(Echo::new())).unwrap();
        let executor = whizbang::wiring::runner(registry, store);
        let request = Request::new(echo("hello")).with_id(RequestId::parse("order-17").unwrap());
        executor
            .request(&request)
            .unwrap()
            .run()
            .as_str()
            .to_owned()
    };
    let shown = whizbang(&["show", &run], &root);
    assert!(
        shown
            .text()
            .starts_with(&format!("run: {run}\nrequest: order-17\n")),
        "{}",
        shown.text()
    );
    let json: serde_json::Value =
        serde_json::from_slice(&whizbang(&["show", &run, "--json"], &root).out).unwrap();
    assert_eq!(json["identity"]["request"], "order-17");
}

#[test]
fn a_run_whose_admission_is_not_one_is_listed_and_shown_with_the_reason() {
    // A run that holds something under `admitted` that this build does not read, and no
    // record: it is not said to have no record and nothing else.
    let dir = Scratch::new("runs");
    let root = dir.path().join("store");
    {
        let store = Store::open(&root, Arc::new(RealFs)).unwrap();
        let mut publication = store.begin().unwrap();
        publication
            .put_in(
                "odd",
                "admitted",
                Kind::Diagnostic,
                b"{\"record\":\"admission/9\"}\n",
            )
            .unwrap();
        publication.publish().unwrap();
    }
    let listed = whizbang(&["list"], &root);
    assert_eq!(
        listed.text().lines().nth(1).unwrap(),
        "odd\tno record; admission not read: not an admission: a record of another kind or \
         version: admission/9"
    );
    let shown = whizbang(&["show", "odd"], &root);
    assert_eq!(shown.code, Some(1));
    assert!(
        shown.err.contains(
            "the run has no record, and its admission is not read: not an admission: a \
             record of another kind or version: admission/9"
        ),
        "{}",
        shown.err
    );
}

/// An action that calls `echo`, and then an action there is none of.
struct Nests(ActionDescriptor);

impl Action for Nests {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }

    fn run(&self, _: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let inner = context.executor().submit(&echo("inside"));
        assert!(inner.is_ok(), "{inner:?}");
        let refused = context.executor().submit(&Call::new("no\u{1b}such"));
        assert!(matches!(refused, Err(CallError::Refused(_))), "{refused:?}");
        Ok(Produced::nothing())
    }
}

#[test]
fn a_run_another_run_called_is_marked_in_the_list_and_each_is_shown_with_the_other() {
    let dir = Scratch::new("runs");
    let root = dir.path().join("store");
    let parent = {
        let store = Arc::new(Store::open(&root, Arc::new(RealFs)).unwrap());
        let mut registry = whizbang::wiring::registry().expect("the registry builds");
        registry.register(Arc::new(Echo::new())).unwrap();
        let kind = whizbang_core::contract::Kind::Primitive;
        let nests = ActionDescriptor::draft("nests", "1.0.0", kind, "A test action.");
        registry
            .register(Arc::new(Nests(nests.build().unwrap())))
            .unwrap();
        let executor = whizbang::wiring::runner(registry, store);
        let finished = executor.submit(&Call::new("nests")).unwrap();
        assert_eq!(finished.outcome().name(), "completed");
        finished.run().as_str().to_owned()
    };

    // The list marks the run that was called, with the run that called it.
    let listed = whizbang(&["list"], &root);
    assert_eq!(listed.code, Some(0), "{}", listed.err);
    let lines: Vec<Vec<&str>> = listed
        .text()
        .lines()
        .skip(1)
        .map(|line| line.split('\t').collect())
        .collect();
    assert_eq!(lines.len(), 2);
    let of = |action: &str| lines.iter().find(|fields| fields[2] == action).unwrap();
    assert_eq!(of("nests 1.0.0").len(), 5);
    assert_eq!(of("nests 1.0.0")[0], parent);
    let child = of("echo 1.0.0")[0].to_owned();
    assert_eq!(of("echo 1.0.0")[5], format!("called by {parent}"));

    // Each is shown with the other: the parent of one, a step of the other. What was
    // refused is shown escaped.
    let shown = whizbang(&["show", &child], &root);
    assert!(shown.text().contains(&format!("\nparent: {parent}\n")));
    assert!(shown.text().contains("\nsteps: none\n"));
    let shown = whizbang(&["show", &parent], &root);
    let text = shown.text();
    assert!(text.contains("\nparent: none\n"), "{text}");
    let step = format!("\nsteps: 1\n  call\t{child}\techo\tcompleted\n");
    assert!(text.contains(&step), "{text}");
    let refusal = "\nrefusals: 1\n  none\tunknown_action\tno action named no\\u{1b}such is \
                   registered\n";
    assert!(text.contains(refusal), "{text}");
    assert!(!shown.out.contains(&0x1b));
}
