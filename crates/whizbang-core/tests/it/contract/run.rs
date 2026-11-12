//! A run's id, and what an action is handed and hands back.

use std::sync::Arc;
use std::time::{Duration, Instant};

use whizbang_core::artifact::{ArtifactId, ArtifactType};
use whizbang_core::contract::{
    Allowance, Call, CallError, Cancellation, Capabilities, Committed, Context, Events, Executor,
    Finished, Outcome, PreparedCall, Refusal, RunId,
};
use whizbang_core::identity::Digest;

#[test]
fn a_run_id_is_a_short_name_safe_in_a_path_and_on_a_command_line() {
    for id in ["r", "run-1", "0", "20261112-a1b2", "a_b-c", &"x".repeat(64)] {
        let parsed = RunId::parse(id).unwrap_or_else(|why| panic!("{id:?} was refused: {why}"));
        assert_eq!(parsed.as_str(), id);
        assert_eq!(parsed.to_string(), id);
    }
    for id in [
        "",
        "Run-1",
        "-run",
        "_run",
        "run 1",
        "run/1",
        "..",
        "run.1",
        "a\nb",
        "a\tb",
        "caf\u{e9}",
        // Upper case after the first character, and digits that are not ASCII, first
        // and later: an Arabic-Indic three and a full-width one.
        "rUn",
        "aB",
        "run-1A",
        "a\u{663}",
        "\u{663}a",
        "run-\u{ff11}",
        &"x".repeat(65),
    ] {
        let error = RunId::parse(id)
            .err()
            .unwrap_or_else(|| panic!("{id:?} was accepted"));
        assert!(error.contains("a run id is"), "{error}");
    }
}

#[test]
fn a_refused_run_id_is_echoed_up_to_the_limit_and_no_further() {
    // What is refused may be anything, of any length: the refusal shows its first 64
    // characters, escaped, and says how long it was.
    let long = format!("Run {}", "y".repeat(5000));
    let error = RunId::parse(&long).unwrap_err();
    assert!(error.len() < 400, "{} bytes", error.len());
    assert!(error.contains(&format!("Run {}", "y".repeat(60))));
    assert!(!error.contains(&"y".repeat(61)));
    assert!(error.contains("5004 characters"), "{error}");
    // A short one is shown whole, and a control character in it escaped.
    let error = RunId::parse("a\nb").unwrap_err();
    assert!(error.starts_with("\"a\\nb\" is no run id"), "{error}");
}

#[test]
fn every_run_id_is_one_the_store_takes() {
    // The store's own rule is the looser one: not empty, no control character. So an id
    // that is a `RunId` is always an id the store publishes under.
    use whizbang_core::store::fs::RealFs;
    use whizbang_core::store::{Kind, Store};
    use whizbang_testkit::scratch::Scratch;
    let dir = Scratch::new("run-id");
    let store = Store::open(&dir.path().join("store"), Arc::new(RealFs)).unwrap();
    let mut publication = store.begin().unwrap();
    for id in ["r", "0", "a_b-c", &"x".repeat(64)] {
        let id = RunId::parse(id).unwrap();
        publication
            .put_in(id.as_str(), "journal", Kind::Diagnostic, b"{}")
            .unwrap();
    }
    publication.publish().unwrap();
    assert_eq!(store.snapshot().unwrap().runs().len(), 4);
}

#[test]
fn a_cancellation_is_seen_by_everything_that_holds_it() {
    let cancellation = Cancellation::new();
    let held = cancellation.clone();
    assert!(!held.is_cancelled());
    cancellation.cancel();
    assert!(held.is_cancelled());
    // Cancelling is final, and doing it twice is doing it once.
    cancellation.cancel();
    assert!(held.is_cancelled());
}

struct Heard(std::sync::Mutex<Vec<String>>);

impl Events for Heard {
    fn progress(&self, message: &str) {
        self.0.lock().unwrap().push(message.to_owned());
    }
}

struct Nothing;

impl Executor for Nothing {
    fn prepare(&self, _: &Call) -> Result<PreparedCall, CallError> {
        Err(Refusal::NoNestedCalls.into())
    }

    fn submit(&self, _: &Call) -> Result<Finished, CallError> {
        Err(Refusal::NoNestedCalls.into())
    }
}

#[test]
fn a_context_carries_what_section_3_lists() {
    use whizbang_core::store::Reader;
    use whizbang_core::store::Store;
    use whizbang_core::store::fs::RealFs;
    use whizbang_testkit::scratch::Scratch;
    let dir = Scratch::new("context");
    let root = dir.path().join("store");
    drop(Store::open(&root, Arc::new(RealFs)).unwrap());
    let cancellation = Cancellation::new();
    let heard = Arc::new(Heard(std::sync::Mutex::new(Vec::new())));
    let deadline = Instant::now() + Duration::from_secs(60);
    let context = Context::new(
        cancellation.clone(),
        Some(deadline),
        heard.clone(),
        Allowance::none(),
        Arc::new(Reader::open(&root, Arc::new(RealFs)).unwrap()),
        Capabilities::none(),
        Arc::new(Nothing),
    );
    // A cancellation signal, shared with whoever cancels.
    assert!(!context.cancellation().is_cancelled());
    cancellation.cancel();
    assert!(context.cancellation().is_cancelled());
    // A deadline, an event sink for progress, a spend allowance.
    assert_eq!(context.deadline(), Some(deadline));
    context.events().progress("half way");
    assert_eq!(*heard.0.lock().unwrap(), ["half way"]);
    let _: &Allowance = context.allowance();
    // The store, to read from.
    assert_eq!(context.store().snapshot().unwrap().number(), 0);
    // Capability handles: none of the three is offered until there are backends.
    assert!(context.capabilities().segment().is_none());
    assert!(context.capabilities().fill().is_none());
    assert!(context.capabilities().judge().is_none());
    // A place for diagnostics: none until the context is given one.
    assert!(context.diagnostics().write("notes.txt", b"x").is_err());
    // And the executor, for a call to another action.
    assert_eq!(
        context.executor().submit(&Call::new("echo")),
        Err(Refusal::NoNestedCalls.into())
    );
}

#[test]
fn an_outcome_is_one_of_four_and_is_handed_back_with_its_run() {
    let output = Committed::new(
        "mask",
        ArtifactType::Mask,
        ArtifactId(Digest::of_bytes(b"a mask")),
    );
    let outcomes = [
        Outcome::Completed {
            outputs: vec![output.clone()],
        },
        Outcome::Cancelled,
        Outcome::Error {
            kind: "backend".to_owned(),
            message: "the filler timed out".to_owned(),
        },
        Outcome::Invalid {
            why: "the mask is empty".to_owned(),
        },
    ];
    let names: Vec<&str> = outcomes.iter().map(Outcome::name).collect();
    assert_eq!(names, ["completed", "cancelled", "error", "invalid"]);
    // Only a completed outcome has outputs.
    assert_eq!(outcomes[0].outputs(), std::slice::from_ref(&output));
    for other in &outcomes[1..] {
        assert!(other.outputs().is_empty());
    }
    assert_eq!(
        (output.port(), output.artifact()),
        ("mask", ArtifactType::Mask)
    );
    let finished = Finished::new(RunId::parse("run-1").unwrap(), Outcome::Cancelled);
    assert_eq!(finished.run().as_str(), "run-1");
    assert_eq!(finished.outcome(), &Outcome::Cancelled);
}
