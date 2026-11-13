//! Running an action through the executor, and what a run leaves in the store (ADR
//! 0005): its record, its journal and its diagnostics, whatever its outcome, published
//! together and durable before the run is reported.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use whizbang_core::artifact::{self, Artifact, ArtifactType, ImageRole};
use whizbang_core::contract::record::{self, RunRecord, Status};
use whizbang_core::contract::{
    Action, ActionDescriptor, Call, CallError, Clock, Context, Engine, Executor, Failure, Finished,
    InputPort, Kind, NoBackends, Outcome, PreparedCall, Produced, Refusal, Registry, RunId,
    Secrets, Value, WHIZBANG_VERSION, is_diagnostic_name, read_journal,
};
use whizbang_core::identity::Digest;
use whizbang_core::store::fs::{Fs, RealFs};
use whizbang_core::store::{self, Reader, Store};
use whizbang_testkit::echo::Echo;
use whizbang_testkit::fs_shim::{Event, Lost, ShimFs};
use whizbang_testkit::schema::validate;
use whizbang_testkit::scratch::Scratch;

use super::record::{completed, schema};
use super::redact::made_up_key;
use crate::artifact::support::{image_of, photo};

/// A clock that says 1000, then 1001, and so on: one more each time it is asked.
struct Ticking(AtomicU64);

impl Clock for Ticking {
    fn now_ms(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst)
    }
}

/// A clock that always says the same.
struct Stopped(u64);

impl Clock for Stopped {
    fn now_ms(&self) -> u64 {
        self.0
    }
}

type Body = Box<dyn Fn(&PreparedCall, &Context) -> Result<Produced, Failure> + Send + Sync>;

/// An action whose run is whatever the test says.
struct Scripted(ActionDescriptor, Body);

impl Action for Scripted {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        (self.1)(call, context)
    }
}

fn scripted(
    name: &str,
    body: impl Fn(&PreparedCall, &Context) -> Result<Produced, Failure> + Send + Sync + 'static,
) -> Arc<dyn Action> {
    let draft = ActionDescriptor::draft(name, "1.0.0", Kind::Primitive, "A test action.");
    Arc::new(Scripted(draft.build().unwrap(), Box::new(body)))
}

/// An action that says what it is and has nothing to run.
struct Described(ActionDescriptor);

impl Action for Described {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }
}

struct World {
    dir: Scratch,
    fs: Arc<ShimFs>,
}

impl World {
    fn new() -> World {
        World {
            dir: Scratch::new("execute"),
            fs: Arc::new(ShimFs::new()),
        }
    }

    fn root(&self) -> std::path::PathBuf {
        self.dir.path().join("store")
    }

    fn store(&self) -> Arc<Store> {
        Arc::new(Store::open(&self.root(), self.fs.clone()).unwrap())
    }

    /// An executor over a store opened to write, with `echo` and these actions, telling
    /// the time from 1000 on.
    fn engine_with(&self, actions: Vec<Arc<dyn Action>>) -> Engine {
        let mut registry = Registry::new();
        registry.register(Arc::new(Echo::new())).unwrap();
        for action in actions {
            registry.register(action).unwrap();
        }
        Engine::over(
            registry,
            Arc::new(NoBackends),
            self.store(),
            Digest::of_bytes(b""),
        )
        .with_clock(Arc::new(Ticking(AtomicU64::new(1000))))
    }

    fn engine(&self) -> Engine {
        self.engine_with(Vec::new())
    }
}

fn echo(text: &str) -> Call {
    Call::new("echo").argument("text", Value::text(text))
}

/// What a run holds in the store: each of its names, with the kind of its reference
/// and its content.
fn names_of(root: &Path, fs: Arc<dyn Fs>, run: &str) -> Vec<(String, store::Kind, Vec<u8>)> {
    let reader = Reader::open(root, fs).unwrap();
    let generation = reader.snapshot().unwrap();
    let run = generation.run(run).unwrap();
    let held = run.entries().iter();
    held.map(|(name, entry)| (name.clone(), entry.kind, reader.read(entry).unwrap()))
        .collect()
}

fn record_of(root: &Path, fs: Arc<dyn Fs>, run: &str) -> RunRecord {
    let reader = Reader::open(root, fs).unwrap();
    let generation = reader.snapshot().unwrap();
    record::stored(&generation, run, |entry| reader.read(entry))
        .unwrap()
        .unwrap()
}

/// The hand-written record of the run `1000-1`, with this build's version where it has
/// the first one's, and this run's id where it has that one: the count in an id is the
/// process's, so a test does not choose it.
fn expected(record: &str, run: &str) -> String {
    assert!(run.starts_with("1000-"), "{run}");
    record
        .replace(
            r#""whizbang":"0.0.1""#,
            &format!(r#""whizbang":"{WHIZBANG_VERSION}""#),
        )
        .replace(r#""run":"1000-1""#, &format!(r#""run":"{run}""#))
}

// --- echo, completed ------------------------------------------------------------------------

#[test]
fn echo_runs_through_the_executor_and_its_record_is_in_the_store() {
    let world = World::new();
    let engine = world.engine();
    let finished = engine.submit(&echo("hello")).unwrap();
    let id = finished.run().as_str();
    assert!(id.starts_with("1000-"), "{id}");
    assert_eq!(
        finished.outcome(),
        &Outcome::Completed {
            outputs: Vec::new()
        }
    );

    // The run holds three names, each a diagnostic reference: what the action wrote, the
    // journal, and the record.
    let held = names_of(&world.root(), world.fs.clone(), finished.run().as_str());
    let names: Vec<(&str, store::Kind)> = held.iter().map(|(n, k, _)| (n.as_str(), *k)).collect();
    assert_eq!(
        names,
        [
            ("diagnostics/echo.txt", store::Kind::Diagnostic),
            ("journal", store::Kind::Diagnostic),
            ("record", store::Kind::Diagnostic),
        ]
    );
    assert_eq!(held[0].2, b"hello");
    // Fixed vectors, written by hand from ADR 0005; the two digests inside the record
    // were computed from these bytes with SHA-256 from another library, apart from this
    // code.
    assert_eq!(
        String::from_utf8(held[1].2.clone()).unwrap(),
        concat!(
            r#"{"seq":0,"at_ms":1001,"message":"echo: hello"}"#,
            "\n",
            r#"{"seq":1,"at_ms":1002,"message":"echo: done"}"#,
            "\n"
        )
    );
    assert_eq!(
        String::from_utf8(held[2].2.clone()).unwrap(),
        expected(&completed(), id)
    );

    // It validates against the record's schema, and reads back as what was reported.
    let document: serde_json::Value = serde_json::from_slice(&held[2].2).unwrap();
    validate(&schema(), &document).unwrap();
    let record = record_of(&world.root(), world.fs.clone(), finished.run().as_str());
    assert_eq!(record.identity().run, finished.run().as_str());
    assert_eq!(record.identity().whizbang, WHIZBANG_VERSION);
    assert_eq!(record.outcome().status, Status::Completed);
    assert_eq!(record.asked().config, Digest::of_bytes(b""));
    // Each diagnostic the record lists is what the run holds under that name.
    for diagnostic in record.diagnostics() {
        let (_, _, bytes) = held
            .iter()
            .find(|(n, _, _)| n == diagnostic.name())
            .unwrap();
        assert_eq!(diagnostic.digest(), Digest::of_bytes(bytes));
        assert_eq!(diagnostic.bytes(), bytes.len() as u64);
    }
    let journal = read_journal(&held[1].2).unwrap();
    assert_eq!(journal.len(), 2);
    assert_eq!(journal[1].message(), "echo: done");
}

#[test]
fn every_group_of_a_real_record_is_filled_or_explicitly_empty() {
    let world = World::new();
    let finished = world.engine().submit(&echo("hello")).unwrap();
    let held = names_of(&world.root(), world.fs.clone(), finished.run().as_str());
    let document: serde_json::Value = serde_json::from_slice(&held[2].2).unwrap();
    let filled = [
        "record",
        "identity",
        "asked",
        "outcome",
        "time",
        "diagnostics",
    ];
    let empty = [
        "inputs", "outputs", "models", "spend", "egress", "steps", "judge",
    ];
    for group in filled {
        let value = &document[group];
        assert!(
            value.is_string()
                || value.as_object().is_some_and(|o| !o.is_empty())
                || value.as_array().is_some_and(|a| !a.is_empty()),
            "{group}: {value}"
        );
    }
    for group in empty {
        assert_eq!(document[group], serde_json::json!([]), "{group}");
    }
    assert_eq!(
        document.as_object().unwrap().len(),
        filled.len() + empty.len()
    );
}

// --- echo, failed ---------------------------------------------------------------------------

#[test]
fn a_failed_echo_writes_the_same_record_with_its_error_and_keeps_its_diagnostics() {
    let world = World::new();
    let engine = world.engine();
    let call = echo("hello").argument("fail", Value::text("boom"));
    let finished = engine.submit(&call).unwrap();
    assert_eq!(
        finished.outcome(),
        &Outcome::Error {
            kind: "echo".to_owned(),
            message: "boom".to_owned()
        }
    );
    assert!(finished.outcome().outputs().is_empty());

    let held = names_of(&world.root(), world.fs.clone(), finished.run().as_str());
    // The same three names, each a diagnostic: what it wrote before it failed is kept.
    let names: Vec<&str> = held.iter().map(|(n, _, _)| n.as_str()).collect();
    assert_eq!(names, ["diagnostics/echo.txt", "journal", "record"]);
    assert!(
        held.iter()
            .all(|(_, kind, _)| *kind == store::Kind::Diagnostic)
    );
    assert_eq!(held[0].2, b"hello");
    assert_eq!(
        String::from_utf8(held[1].2.clone()).unwrap(),
        concat!(r#"{"seq":0,"at_ms":1001,"message":"echo: hello"}"#, "\n")
    );
    // The record, by hand: the same shape, the error in its place, no output and no
    // completed status.
    let by_hand = concat!(
        r#"{"record":"run/1","#,
        r#""identity":{"run":"1000-1","request":null,"parent":null,"whizbang":"0.0.1"},"#,
        r#""asked":{"action":"echo","version":"1.0.0","#,
        r#""arguments":[{"name":"text","value":{"text":"hello"}},"#,
        r#"{"name":"fail","value":{"text":"boom"}}],"settings":[],"#,
        r#""config":"sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"},"#,
        r#""inputs":[],"outputs":[],"#,
        r#""outcome":{"status":"error","error":{"kind":"echo","message":"boom"},"#,
        r#""invalid":null,"refusals":[]},"#,
        r#""time":{"start_ms":1000,"end_ms":1002,"durations":[]},"#,
        r#""models":[],"spend":[],"egress":[],"steps":[],"judge":[],"#,
        r#""diagnostics":[{"name":"diagnostics/echo.txt","#,
        r#""digest":"sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824","#,
        r#""bytes":5},{"name":"journal","#,
        r#""digest":"sha256:ebc4602b6688fa7255b1aed43d9fbd0258c94df1d5d6285bf1773171af58f016","#,
        r#""bytes":47}]}"#,
        "\n"
    );
    let written = String::from_utf8(held[2].2.clone()).unwrap();
    assert_eq!(written, expected(by_hand, finished.run().as_str()));
    assert!(!written.contains("completed"));
    validate(&schema(), &serde_json::from_str(&written).unwrap()).unwrap();

    // Readable, and never usable as an output: a diagnostic is refused where an output
    // is asked for.
    let reader = Reader::open(&world.root(), world.fs.clone()).unwrap();
    let generation = reader.snapshot().unwrap();
    let run = generation.run(finished.run().as_str()).unwrap();
    for entry in run.entries().values() {
        reader.read(entry).unwrap();
        assert!(matches!(
            reader.read_output(entry),
            Err(store::Error::NotAnOutput { .. })
        ));
    }
    // No artifact was published by the run.
    assert!(generation.entries().is_empty());
}

#[test]
fn an_echo_that_finds_its_text_invalid_is_an_invalid_run_with_its_record() {
    let world = World::new();
    let call = echo("hello").argument("invalid", Value::text("the text is too short"));
    let finished = world.engine().submit(&call).unwrap();
    assert_eq!(
        finished.outcome(),
        &Outcome::Invalid {
            why: "the text is too short".to_owned()
        }
    );
    let record = record_of(&world.root(), world.fs.clone(), finished.run().as_str());
    assert_eq!(record.outcome().status, Status::Invalid);
    assert_eq!(
        record.outcome().invalid.as_deref(),
        Some("the text is too short")
    );
    assert!(record.outcome().error.is_none());
}

#[test]
fn an_action_with_nothing_to_run_fails_and_has_its_record() {
    let world = World::new();
    let draft = ActionDescriptor::draft("only_described", "1.0.0", Kind::Primitive, "Says it.");
    let engine = world.engine_with(vec![Arc::new(Described(draft.build().unwrap()))]);
    let finished = engine.submit(&Call::new("only_described")).unwrap();
    let Outcome::Error { kind, message } = finished.outcome() else {
        panic!("{finished:?}");
    };
    assert_eq!(kind, "not_runnable");
    assert!(message.contains("only_described has nothing to run"));
    let record = record_of(&world.root(), world.fs.clone(), finished.run().as_str());
    assert_eq!(record.outcome().status, Status::Error);
}

// --- a refusal is not a run -----------------------------------------------------------------

#[test]
fn a_refused_call_makes_no_run_and_writes_nothing() {
    let world = World::new();
    let engine = world.engine();
    let events = world.fs.events().len();
    assert!(matches!(
        engine.submit(&Call::new("vanish")),
        Err(CallError::Refused(Refusal::UnknownAction { .. }))
    ));
    assert!(matches!(
        engine.submit(&Call::new("echo")),
        Err(CallError::Refused(Refusal::Invalid { .. }))
    ));
    assert_eq!(world.fs.events().len(), events);
    let reader = Reader::open(&world.root(), world.fs.clone()).unwrap();
    assert_eq!(reader.snapshot().unwrap().number(), 0);
}

// --- what an action is handed ---------------------------------------------------------------

#[test]
fn an_action_is_handed_a_context_and_cannot_yet_call_another_action() {
    let world = World::new();
    let engine = world.engine_with(vec![scripted("nest", |_, context| {
        assert!(!context.cancellation().is_cancelled());
        assert!(context.deadline().is_none());
        assert!(context.capabilities().segment().is_none());
        // The store can be read from inside a run.
        assert_eq!(context.store().snapshot().unwrap().number(), 0);
        // A call to another action is refused, by name, and prepared no further.
        let refused = context.executor().submit(&echo("inner")).unwrap_err();
        assert_eq!(refused, CallError::Refused(Refusal::NoNestedCalls));
        assert_eq!(
            context.executor().prepare(&echo("inner")).unwrap_err(),
            CallError::Refused(Refusal::NoNestedCalls)
        );
        Err(Failure::error("nested", &refused.to_string()))
    })]);
    let finished = engine.submit(&Call::new("nest")).unwrap();
    let Outcome::Error { kind, message } = finished.outcome() else {
        panic!("{finished:?}");
    };
    assert_eq!(kind, "nested");
    assert!(message.contains("does not nest calls"), "{message}");
    // One run: the inner call made none.
    let reader = Reader::open(&world.root(), world.fs.clone()).unwrap();
    assert_eq!(reader.snapshot().unwrap().runs().len(), 1);
}

#[test]
fn a_diagnostic_has_a_plain_name_and_is_written_once() {
    for name in [
        "a",
        "echo.txt",
        "turn-3.exchange.json",
        "0",
        &"x".repeat(64),
    ] {
        assert!(is_diagnostic_name(name), "{name}");
    }
    for name in [
        "",
        ".hidden",
        "..",
        "a/b",
        "A.txt",
        "a b",
        "a\nb",
        "caf\u{e9}",
        "-a",
        &"x".repeat(65),
    ] {
        assert!(!is_diagnostic_name(name), "{name:?}");
    }
    let world = World::new();
    let engine = world.engine_with(vec![scripted("writes", |_, context| {
        let diagnostics = context.diagnostics();
        diagnostics.write("first.txt", b"one").unwrap();
        let again = diagnostics.write("first.txt", b"two").unwrap_err();
        assert!(again.contains("is written once"), "{again}");
        let long = format!("../{}", "y".repeat(500));
        let refused = diagnostics.write(&long, b"x").unwrap_err();
        assert!(refused.contains("is no name for a diagnostic"), "{refused}");
        assert!(refused.len() < 300, "{refused}");
        diagnostics
            .write("picture.bin", &[0xff, 0x00, 0xfe])
            .unwrap();
        Ok(Produced::nothing())
    })]);
    let finished = engine.submit(&Call::new("writes")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    let held = names_of(&world.root(), world.fs.clone(), finished.run().as_str());
    let kept: Vec<(&str, &[u8])> = held
        .iter()
        .map(|(n, _, b)| (n.as_str(), b.as_slice()))
        .collect();
    assert_eq!(kept[0], ("diagnostics/first.txt", &b"one"[..]));
    assert_eq!(
        kept[1],
        ("diagnostics/picture.bin", &[0xff, 0x00, 0xfe][..])
    );
    assert_eq!(kept[2], ("journal", &b""[..]));
    assert_eq!(kept.len(), 4);
}

#[test]
fn the_inputs_of_a_run_are_recorded_with_the_binding_each_belongs_to() {
    let world = World::new();
    let image = image_of(photo(), ImageRole::Picture);
    let draft = ActionDescriptor::draft("look", "1.0.0", Kind::Primitive, "Looks.").input(
        InputPort::required("image", ArtifactType::Image, "The image."),
    );
    let look = Scripted(
        draft.build().unwrap(),
        Box::new(|_, _| Ok(Produced::nothing())),
    );
    let engine = world.engine_with(vec![Arc::new(look)]);
    {
        let store = Store::open_existing(&world.root(), world.fs.clone());
        // The engine holds the store: a second writer is refused, here as everywhere.
        assert!(matches!(store, Err(store::Error::Held { .. })));
    }
    drop(engine);
    let store = world.store();
    let mut publication = store.begin().unwrap();
    artifact::put(&mut publication, &Artifact::from(image.clone())).unwrap();
    publication.publish().unwrap();
    drop(store);
    let draft = ActionDescriptor::draft("look", "1.0.0", Kind::Primitive, "Looks.").input(
        InputPort::required("image", ArtifactType::Image, "The image."),
    );
    let look = Scripted(
        draft.build().unwrap(),
        Box::new(|_, _| Ok(Produced::nothing())),
    );
    let engine = world.engine_with(vec![Arc::new(look)]);
    let finished = engine
        .submit(&Call::new("look").input("image", image.id()))
        .unwrap();
    let record = record_of(&world.root(), world.fs.clone(), finished.run().as_str());
    assert_eq!(record.inputs().len(), 1);
    let input = &record.inputs()[0];
    assert_eq!(input.port, "image");
    assert_eq!(input.artifact_type, ArtifactType::Image);
    assert_eq!(input.artifact, image.id().0);
    assert_eq!(input.binding, image.binding().id().0);
    let held = names_of(&world.root(), world.fs.clone(), finished.run().as_str());
    let document: serde_json::Value = serde_json::from_slice(&held.last().unwrap().2).unwrap();
    validate(&schema(), &document).unwrap();
}

// --- run ids ----------------------------------------------------------------------------------

#[test]
fn every_run_gets_an_id_of_its_own_that_is_a_run_id() {
    // With a clock that does not move, and with a run of the first id already in the
    // store, made by other hands.
    let world = World::new();
    let store = world.store();
    let mut publication = store.begin().unwrap();
    publication
        .put_in("5000-1", "notes", store::Kind::Diagnostic, b"x")
        .unwrap();
    publication.publish().unwrap();
    let mut registry = Registry::new();
    registry.register(Arc::new(Echo::new())).unwrap();
    let engine = Engine::over(registry, Arc::new(NoBackends), store, Digest::of_bytes(b""))
        .with_clock(Arc::new(Stopped(5000)));
    let ids: Vec<String> = (0..3)
        .map(|_| engine.submit(&echo("x")).unwrap().run().as_str().to_owned())
        .collect();
    // Three ids at one instant: each its own, counted upwards, and none the id that was
    // already there.
    let counts: Vec<u64> = ids
        .iter()
        .map(|id| id.strip_prefix("5000-").unwrap().parse().unwrap())
        .collect();
    assert!(
        counts[0] > 1 && counts[0] < counts[1] && counts[1] < counts[2],
        "{ids:?}"
    );
    for id in &ids {
        assert_eq!(RunId::parse(id).unwrap().as_str(), id);
    }
    // The run that was there is as it was.
    let held = names_of(&world.root(), world.fs.clone(), "5000-1");
    assert_eq!(held.len(), 1);
    // The largest time there is still makes a run id.
    let last = Engine::over(
        {
            let mut registry = Registry::new();
            registry.register(Arc::new(Echo::new())).unwrap();
            registry
        },
        Arc::new(NoBackends),
        {
            drop(engine);
            world.store()
        },
        Digest::of_bytes(b""),
    )
    .with_clock(Arc::new(Stopped(u64::MAX)));
    let id = last.submit(&echo("x")).unwrap().run().as_str().to_owned();
    assert!(id.starts_with(&format!("{}-", u64::MAX)), "{id}");
    RunId::parse(&id).unwrap();
}

// --- durable before reported, and whole or not at all ----------------------------------------

#[test]
fn a_run_that_was_reported_survives_a_loss_of_power() {
    let world = World::new();
    let engine = world.engine();
    let finished = engine.submit(&echo("hello")).unwrap();
    // Power is lost the moment the run was reported: everything not yet durable goes.
    world.fs.crash(Lost::Unsynced);
    drop(engine);
    let record = record_of(&world.root(), Arc::new(RealFs), finished.run().as_str());
    assert_eq!(record.outcome().status, Status::Completed);
    let held = names_of(&world.root(), Arc::new(RealFs), finished.run().as_str());
    assert_eq!(held.len(), 3);
}

#[test]
fn a_run_whose_publication_fails_is_reported_as_the_stores_failure_and_leaves_no_run() {
    // The store fails at each step of the run's publication in turn. The answer is the
    // store's failure, never a finished run, and the store holds no part of the run.
    let steps = {
        let world = World::new();
        let engine = world.engine();
        let before = world.fs.events().len();
        engine.submit(&echo("hello")).unwrap();
        world.fs.events()[before..].to_vec()
    };
    assert!(steps.len() > 5, "{steps:?}");
    for (step, event) in steps.iter().enumerate() {
        let world = World::new();
        let engine = world.engine();
        world.fs.refuse_before(world.fs.events().len() + step);
        let result = engine.submit(&echo("hello"));
        let reader = Reader::open(&world.root(), world.fs.clone()).unwrap();
        let generation = reader.snapshot().unwrap();
        let said = match result {
            Err(CallError::Store(failure)) => CallError::Store(failure).to_string(),
            // Clearing away the staging, after the publication is durable, can fail
            // without failing the run: the run is whole and was reported.
            Ok(finished) if matches!(event, Event::Remove(_)) => {
                let run = generation.run(finished.run().as_str()).unwrap();
                assert_eq!(run.entries().len(), 3);
                continue;
            }
            other => panic!("step {step}, {event:?}: {other:?}"),
        };
        assert!(!said.contains(world.root().to_str().unwrap()), "{said}");
        if generation.number() == 0 {
            assert!(generation.runs().is_empty(), "step {step}");
        } else {
            // Visible and not confirmed durable: the run is whole, and it was not
            // reported as finished.
            assert!(
                said.contains("not confirmed durable"),
                "step {step}: {said}"
            );
            let runs: Vec<usize> = generation
                .runs()
                .values()
                .map(|r| r.entries().len())
                .collect();
            assert_eq!(runs, [3], "step {step}");
        }
    }
}

#[test]
fn the_record_is_published_in_one_generation_with_the_journal_and_the_diagnostics() {
    let world = World::new();
    let engine = world.engine();
    engine.submit(&echo("hello")).unwrap();
    let reader = Reader::open(&world.root(), world.fs.clone()).unwrap();
    assert_eq!(reader.snapshot().unwrap().number(), 1);
    engine.submit(&echo("again")).unwrap();
    assert_eq!(reader.snapshot().unwrap().number(), 2);
    // One pointer replaced for each run.
    let pointers = world
        .fs
        .events()
        .into_iter()
        .filter(|event| matches!(event, Event::Rename(_, to) if to.ends_with("CURRENT")));
    // The store's creation placed the first pointer.
    assert_eq!(pointers.count(), 3);
}

// --- nothing private --------------------------------------------------------------------------

/// Every file under a directory, with its bytes.
fn files(root: &Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    let mut found = Vec::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else {
                found.push((path.clone(), std::fs::read(&path).unwrap()));
            }
        }
    }
    found
}

fn holds(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[test]
fn a_secret_given_to_a_run_is_in_nothing_the_store_holds() {
    // A made-up key, planted everywhere a run takes text from outside: an argument, the
    // progress event and the diagnostic made from it, an error's message, and a
    // diagnostic that quotes it inside JSON.
    let key = made_up_key("");
    let quoted = "pa\"ss\\word".to_owned();
    let world = World::new();
    let leaky = {
        let (key, quoted) = (key.clone(), quoted.clone());
        scripted("leaky", move |_, context| {
            context.events().progress(&format!("sending {key}"));
            let body = serde_json::json!({ "authorization": key, "other": quoted }).to_string();
            context
                .diagnostics()
                .write("exchange.json", body.as_bytes())
                .unwrap();
            Err(Failure::Error {
                kind: format!("auth {key}"),
                message: format!("the service refused {key}"),
            })
        })
    };
    let engine = world
        .engine_with(vec![leaky])
        .with_secrets(Secrets::new([key.clone(), quoted.clone()]).unwrap());
    let first = engine
        .submit(&echo(&format!("my key is {key}")).argument("fail", Value::text(&key)))
        .unwrap();
    let second = engine.submit(&Call::new("leaky")).unwrap();
    drop(engine);

    for (path, bytes) in files(&world.root()) {
        assert!(!holds(&bytes, key.as_bytes()), "{}", path.display());
        assert!(!holds(&bytes, b"ss\\\\word"), "{}", path.display());
    }
    // What stands in its place, in the record and in what was kept beside it.
    let record = record_of(&world.root(), world.fs.clone(), first.run().as_str());
    let written = String::from_utf8(record.to_bytes()).unwrap();
    assert!(
        written.contains(r#"{"text":"my key is [redacted]"}"#),
        "{written}"
    );
    assert_eq!(
        record.outcome().error.as_ref().unwrap().message,
        "[redacted]"
    );
    let held = names_of(&world.root(), world.fs.clone(), first.run().as_str());
    assert_eq!(held[0].2, b"my key is [redacted]");
    assert!(holds(&held[1].2, b"echo: my key is [redacted]"));
    let record = record_of(&world.root(), world.fs.clone(), second.run().as_str());
    let error = record.outcome().error.as_ref().unwrap();
    assert_eq!(error.kind, "auth [redacted]");
    assert_eq!(error.message, "the service refused [redacted]");
    let held = names_of(&world.root(), world.fs.clone(), second.run().as_str());
    let exchange: serde_json::Value = serde_json::from_slice(&held[0].2).unwrap();
    assert_eq!(exchange["authorization"], "[redacted]");
    assert_eq!(exchange["other"], "[redacted]");
    // What the caller is handed back holds no secret either.
    for finished in [&first, &second] {
        let Outcome::Error { kind, message } = finished.outcome() else {
            panic!("{finished:?}");
        };
        assert!(
            !kind.contains(&key) && !message.contains(&key),
            "{finished:?}"
        );
    }
    assert_eq!(
        second.outcome(),
        &Outcome::Error {
            kind: "auth [redacted]".to_owned(),
            message: "the service refused [redacted]".to_owned()
        }
    );
}

#[test]
fn a_record_holds_no_path_of_this_machine_and_nothing_of_its_environment() {
    let world = World::new();
    let engine = world.engine();
    let finished = engine.submit(&echo("hello")).unwrap();
    let held = names_of(&world.root(), world.fs.clone(), finished.run().as_str());
    let private: Vec<String> = [
        Some(world.root().display().to_string()),
        Some(std::env::temp_dir().display().to_string()),
        std::env::var("HOME").ok(),
        std::env::var("USER").ok(),
        std::env::current_dir()
            .ok()
            .map(|dir| dir.display().to_string()),
    ]
    .into_iter()
    .flatten()
    .filter(|value| value.len() > 2)
    .collect();
    assert!(private.len() >= 2);
    for (name, _, bytes) in &held {
        for value in &private {
            assert!(!holds(bytes, value.as_bytes()), "{name} holds {value}");
        }
    }
}

#[test]
fn what_submit_hands_back_names_the_run() {
    let world = World::new();
    let finished: Finished = world.engine().submit(&echo("hello")).unwrap();
    assert!(finished.run().to_string().starts_with("1000-"));
}

#[test]
fn a_run_that_failed_on_a_store_failure_it_was_handed_holds_no_path_in_its_record() {
    // A call's error whose cause was the store's failure, as an action will be handed
    // one when calls nest. The action quotes it both ways, as its text and as `{:?}`,
    // in its own error and in a diagnostic. Neither way writes where a store is.
    let broken = World::new();
    let image = image_of(photo(), ImageRole::Picture);
    {
        let store = broken.store();
        let mut publication = store.begin().unwrap();
        artifact::put(&mut publication, &Artifact::from(image.clone())).unwrap();
        publication.publish().unwrap();
    }
    for entry in std::fs::read_dir(broken.root().join("objects")).unwrap() {
        std::fs::remove_file(entry.unwrap().path()).unwrap();
    }
    let draft = ActionDescriptor::draft("look", "1.0.0", Kind::Primitive, "Looks.").input(
        InputPort::required("image", ArtifactType::Image, "The image."),
    );
    let looker = broken.engine_with(vec![Arc::new(Described(draft.build().unwrap()))]);
    let error = looker
        .submit(&Call::new("look").input("image", image.id()))
        .unwrap_err();
    assert!(matches!(error, CallError::Store(_)), "{error:?}");

    let world = World::new();
    let engine = world.engine_with(vec![scripted("relay", move |_, context| {
        let quoted = format!("{error} / {error:?}");
        context
            .diagnostics()
            .write("inner-error.txt", quoted.as_bytes())
            .unwrap();
        Err(Failure::error("store", &quoted))
    })]);
    let finished = engine.submit(&Call::new("relay")).unwrap();
    let Outcome::Error { message, .. } = finished.outcome() else {
        panic!("{finished:?}");
    };
    assert!(message.contains("the store failed"), "{message}");
    let private = [
        broken.root().display().to_string(),
        std::env::temp_dir().display().to_string(),
    ];
    for (name, _, bytes) in names_of(&world.root(), world.fs.clone(), finished.run().as_str()) {
        for path in &private {
            assert!(!holds(&bytes, path.as_bytes()), "{name} holds {path}");
        }
        assert!(
            !holds(&bytes, b"objects/") && !holds(&bytes, b"objects\\"),
            "{name}"
        );
    }
}

// --- one id, one run -------------------------------------------------------------------------

/// Every run of a store, with how many names it holds.
fn runs_of(root: &Path, fs: Arc<dyn Fs>) -> Vec<(String, usize)> {
    let reader = Reader::open(root, fs).unwrap();
    let generation = reader.snapshot().unwrap();
    let runs = generation.runs().iter();
    runs.map(|(id, run)| (id.clone(), run.entries().len()))
        .collect()
}

fn engine_over(store: Arc<Store>, actions: Vec<Arc<dyn Action>>, now: u64) -> Engine {
    let mut registry = Registry::new();
    registry.register(Arc::new(Echo::new())).unwrap();
    for action in actions {
        registry.register(action).unwrap();
    }
    Engine::over(registry, Arc::new(NoBackends), store, Digest::of_bytes(b""))
        .with_clock(Arc::new(Stopped(now)))
}

#[test]
fn two_executors_over_one_store_never_give_two_runs_one_id() {
    // Two executors share a store and a clock that does not move. Each names its run,
    // and each action then waits for the other, so both runs are named before either is
    // published. Each run has an id of its own and is in the store whole.
    let world = World::new();
    let store = world.store();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let waiting = |name: &str| {
        let barrier = barrier.clone();
        scripted(name, move |_, context| {
            context.diagnostics().write("mine.txt", b"x").unwrap();
            barrier.wait();
            Ok(Produced::nothing())
        })
    };
    let first = engine_over(store.clone(), vec![waiting("wait")], 7000);
    let second = engine_over(store.clone(), vec![waiting("wait")], 7000);
    let (a, b) = std::thread::scope(|scope| {
        let a = scope.spawn(|| first.submit(&Call::new("wait")).unwrap());
        let b = scope.spawn(|| second.submit(&Call::new("wait")).unwrap());
        (a.join().unwrap(), b.join().unwrap())
    });
    assert_ne!(a.run(), b.run());
    let runs = runs_of(&world.root(), world.fs.clone());
    assert_eq!(runs.len(), 2, "{runs:?}");
    for finished in [&a, &b] {
        // The journal, the diagnostic and the record: three names, and the record is
        // this run's own.
        let held = names_of(&world.root(), world.fs.clone(), finished.run().as_str());
        assert_eq!(held.len(), 3, "{}", finished.run());
        let record = record_of(&world.root(), world.fs.clone(), finished.run().as_str());
        assert_eq!(record.identity().run, finished.run().as_str());
    }
}

#[test]
fn many_runs_from_two_executors_at_one_instant_are_all_kept() {
    // Eight threads, two executors, one store, a clock that does not move: two hundred
    // submits. Every reported run is in the store under an id of its own.
    let world = World::new();
    let store = world.store();
    let engines = [
        engine_over(store.clone(), Vec::new(), 9000),
        engine_over(store.clone(), Vec::new(), 9000),
    ];
    let reported: Vec<String> = std::thread::scope(|scope| {
        let threads: Vec<_> = (0..8)
            .map(|thread| {
                let engine = &engines[thread % 2];
                scope.spawn(move || {
                    (0..25)
                        .map(|n| {
                            let finished = engine.submit(&echo(&format!("{thread}-{n}")));
                            finished.unwrap().run().as_str().to_owned()
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        threads
            .into_iter()
            .flat_map(|thread| thread.join().unwrap())
            .collect()
    });
    assert_eq!(reported.len(), 200);
    let distinct: std::collections::BTreeSet<&String> = reported.iter().collect();
    assert_eq!(distinct.len(), 200);
    let runs = runs_of(&world.root(), world.fs.clone());
    assert_eq!(runs.len(), 200);
    assert!(runs.iter().all(|(_, names)| *names == 3), "{runs:?}");
}

/// The count in a run's id: the number after the hyphen.
fn count_of(run: &RunId) -> u64 {
    run.as_str().split_once('-').unwrap().1.parse().unwrap()
}

#[test]
fn a_run_whose_id_was_taken_while_it_executed_is_named_again_and_merges_into_nothing() {
    // The id is free when the run is named. While the action runs, other hands publish
    // runs under that id and the ids after it. The store refuses to make a run that it
    // already holds, the executor names the run again, and nothing is merged: the runs
    // that were planted hold what they held, and the reported run is whole.
    //
    // The count in an id is one for the whole process, and under `cargo test` other
    // tests of this binary name runs too. So the ids to plant are found, not assumed: a
    // first run says where the count is, and the 500 ids after it are planted, which is
    // far more than the other tests name in the moment between.
    const PLANTED: u64 = 500;
    let world = World::new();
    let store = world.store();
    let first = Arc::new(AtomicU64::new(0));
    let planting = {
        let (store, first) = (store.clone(), first.clone());
        scripted("plant", move |_, context| {
            context.diagnostics().write("mine.txt", b"x").unwrap();
            let from = first.load(Ordering::SeqCst);
            let mut publication = store.begin().unwrap();
            for count in from..from + PLANTED {
                publication
                    .put_in(
                        &format!("8000-{count}"),
                        "theirs",
                        store::Kind::Diagnostic,
                        b"y",
                    )
                    .unwrap();
            }
            publication.publish().unwrap();
            Ok(Produced::nothing())
        })
    };
    let engine = engine_over(store.clone(), vec![planting], 8000);
    let before = engine
        .submit(&Call::new("echo").argument("text", Value::text("hello")))
        .unwrap();
    let from = count_of(before.run()) + 1;
    first.store(from, Ordering::SeqCst);

    let finished = engine.submit(&Call::new("plant")).unwrap();
    // The id it was first given was among those planted, so it was refused and named
    // again: the id it ended with is past every planted one.
    assert!(
        count_of(finished.run()) >= from + PLANTED,
        "{} was never taken: no collision was made",
        finished.run()
    );
    let runs = runs_of(&world.root(), world.fs.clone());
    assert_eq!(runs.len() as u64, PLANTED + 2, "{runs:?}");
    for (id, names) in &runs {
        if id == finished.run().as_str() || id == before.run().as_str() {
            assert_eq!(*names, 3, "{id}");
        } else {
            assert_eq!(*names, 1, "{id} was merged into");
        }
    }
    let record = record_of(&world.root(), world.fs.clone(), finished.run().as_str());
    assert_eq!(record.identity().run, finished.run().as_str());
}

/// A file system that takes a run's id from under the executor: each time a run record
/// is about to be written, it first publishes a run of the id the record names, as
/// other hands would, for as many times as it was told to.
struct Taker {
    inner: RealFs,
    store: std::sync::OnceLock<Arc<Store>>,
    left: AtomicU64,
    taken: std::sync::Mutex<Vec<String>>,
}

impl Taker {
    fn take(&self, bytes: &[u8]) {
        let Ok(record) = serde_json::from_slice::<serde_json::Value>(bytes) else {
            return;
        };
        let (Some(id), Some(store)) = (record["identity"]["run"].as_str(), self.store.get()) else {
            return;
        };
        let mut taken = self.taken.lock().unwrap();
        if taken.iter().any(|held| held == id) || self.left.load(Ordering::SeqCst) == 0 {
            return;
        }
        self.left.fetch_sub(1, Ordering::SeqCst);
        taken.push(id.to_owned());
        drop(taken);
        let mut publication = store.begin().unwrap();
        publication
            .put_in(id, "theirs", store::Kind::Diagnostic, b"y")
            .unwrap();
        publication.publish().unwrap();
    }
}

impl Fs for Taker {
    fn create_dir(&self, path: &Path) -> std::io::Result<()> {
        self.inner.create_dir(path)
    }
    fn write_new(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        self.take(bytes);
        self.inner.write_new(path, bytes)
    }
    fn sync_dir(&self, path: &Path) -> std::io::Result<()> {
        self.inner.sync_dir(path)
    }
    fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        self.inner.rename(from, to)
    }
    fn remove_all(&self, path: &Path) -> std::io::Result<()> {
        self.inner.remove_all(path)
    }
    fn read(&self, path: &Path) -> std::io::Result<Vec<u8>> {
        self.inner.read(path)
    }
    fn list(&self, path: &Path) -> std::io::Result<Vec<std::path::PathBuf>> {
        self.inner.list(path)
    }
    fn exists(&self, path: &Path) -> std::io::Result<bool> {
        self.inner.exists(path)
    }
    fn hold_lock_file(
        &self,
        path: &Path,
        create: bool,
    ) -> std::io::Result<Option<Box<dyn store::fs::LockFile>>> {
        self.inner.hold_lock_file(path, create)
    }
    fn share(&self, path: &Path) -> std::io::Result<Box<dyn store::fs::Held>> {
        self.inner.share(path)
    }
    fn try_alone(&self, path: &Path) -> std::io::Result<Option<Box<dyn store::fs::Held>>> {
        self.inner.try_alone(path)
    }
}

#[test]
fn a_run_is_named_eight_times_at_most() {
    // Each time the executor is about to publish its run, the id it chose is taken. It
    // names the run again, and does so until it has named it eight times: taken seven
    // times, the eighth name is published; taken eight times, a ninth name is not
    // tried, and the store's refusal is what the caller is told.
    let submit = |takes: u64| {
        let dir = Scratch::new("execute-namings");
        let root = dir.path().join("store");
        let fs = Arc::new(Taker {
            inner: RealFs,
            store: std::sync::OnceLock::new(),
            left: AtomicU64::new(takes),
            taken: std::sync::Mutex::new(Vec::new()),
        });
        let store = Arc::new(Store::open(&root, fs.clone()).unwrap());
        assert!(fs.store.set(store.clone()).is_ok());
        let engine = engine_over(store, Vec::new(), 9000);
        let result = engine.submit(&Call::new("echo").argument("text", Value::text("hello")));
        let taken = fs.taken.lock().unwrap().clone();
        let runs = runs_of(&root, Arc::new(RealFs));
        (result, taken, runs)
    };

    let (result, taken, runs) = submit(7);
    let finished = result.unwrap();
    assert_eq!(taken.len(), 7);
    assert!(!taken.iter().any(|id| id == finished.run().as_str()));
    // Seven runs of other hands, each with its one name, and the run itself, whole.
    assert_eq!(runs.len(), 8, "{runs:?}");
    for (id, names) in &runs {
        let own = id == finished.run().as_str();
        assert_eq!(*names, if own { 3 } else { 1 }, "{id}");
    }

    let (result, taken, runs) = submit(8);
    assert_eq!(taken.len(), 8);
    match result {
        Err(CallError::Store(failure)) => assert!(
            matches!(failure.cause(), store::Error::RunExists { run } if *run == taken[7]),
            "{:?}",
            failure.cause()
        ),
        other => panic!("a ninth name was tried: {other:?}"),
    }
    // Nothing of the run was published: the eight are the others' alone.
    assert_eq!(runs.len(), 8, "{runs:?}");
    assert!(runs.iter().all(|(_, names)| *names == 1), "{runs:?}");
}

// --- a secret as a name -----------------------------------------------------------------------

#[test]
fn a_secret_is_not_written_as_a_diagnostics_name() {
    // A name is a key in the store: it is in the generation file and in the record, and
    // `runs show` prints it. A secret made of the letters a name may hold is refused as
    // a name, alone or as part of one, and the refusal does not show it.
    let key = format!("sk{}abcdefghij0123456789", "-");
    assert!(is_diagnostic_name(&key));
    let world = World::new();
    let named = {
        let key = key.clone();
        scripted("named", move |_, context| {
            for name in [key.clone(), format!("turn.{key}.txt")] {
                let refused = context.diagnostics().write(&name, b"body").unwrap_err();
                assert!(!refused.contains(&key), "{refused}");
                assert!(refused.contains("must not be written"), "{refused}");
            }
            context.diagnostics().write("fine.txt", b"body").unwrap();
            Ok(Produced::nothing())
        })
    };
    let engine = world
        .engine_with(vec![named])
        .with_secrets(Secrets::new([key.clone()]).unwrap());
    let finished = engine.submit(&Call::new("named")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    drop(engine);
    for (path, bytes) in files(&world.root()) {
        assert!(!holds(&bytes, key.as_bytes()), "{}", path.display());
    }
    let held = names_of(&world.root(), world.fs.clone(), finished.run().as_str());
    assert_eq!(held[0].0, "diagnostics/fine.txt");
    assert_eq!(held.len(), 3);
}

#[test]
fn the_reason_a_run_was_invalid_is_handed_back_without_a_secret() {
    let key = made_up_key("");
    let world = World::new();
    let engine = world
        .engine()
        .with_secrets(Secrets::new([key.clone()]).unwrap());
    let call = echo("hello").argument("invalid", Value::text(&format!("{key} is no prompt")));
    let finished = engine.submit(&call).unwrap();
    assert_eq!(
        finished.outcome(),
        &Outcome::Invalid {
            why: "[redacted] is no prompt".to_owned()
        }
    );
}
