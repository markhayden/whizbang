//! Request ids, and the admission of a run (ADR 0007): an id is looked up before
//! anything is prepared; one publication makes the run and the id's record; the record
//! outlives the run; and the same request sent again is answered with the run it made.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier, Mutex, mpsc};

use whizbang_core::artifact::ArtifactId;
use whizbang_core::contract::record::{self, Status};
use whizbang_core::contract::request::{StoredAdmissionError, admitted};
use whizbang_core::contract::{
    ADMITTED_NAME, Action, ActionDescriptor, Admission, Answer, Argument, Backends, Call,
    CallError, Capability, Clock, Context, Ending, Engine, Executor, Failure, Kind, ModelIdentity,
    NoBackends, Outcome, PreparedCall, Produced, REQUEST_ID_LIMIT, REQUEST_RECORD_MAX_BYTES,
    Refusal, Registry, Request, RequestId, RequestRecordError, Requirement, RunState, Secrets,
    Unavailable, Value, ValueType, request_fingerprint,
};
use whizbang_core::identity::Digest;
use whizbang_core::store::fs::{Fs, RealFs};
use whizbang_core::store::{self, Reader, Store};
use whizbang_testkit::echo::Echo;
use whizbang_testkit::fs_shim::{Lost, ShimFs};
use whizbang_testkit::scratch::Scratch;

use super::redact::made_up_key;

// --- what the tests are made of ----------------------------------------------------------------

/// A clock that always says the same.
struct Stopped(u64);

impl Clock for Stopped {
    fn now_ms(&self) -> u64 {
        self.0
    }
}

/// A clock that does not move, and that holds the first callers until as many have
/// asked as the barrier takes: a request asks the time once it was looked up and
/// prepared, so each of them has looked before any of them is admitted.
struct Together {
    barrier: Barrier,
    left: Mutex<usize>,
}

impl Together {
    fn new(callers: usize) -> Together {
        Together {
            barrier: Barrier::new(callers),
            left: Mutex::new(callers),
        }
    }
}

impl Clock for Together {
    fn now_ms(&self) -> u64 {
        let waits = {
            let mut left = self.left.lock().unwrap();
            let waits = *left > 0;
            *left = left.saturating_sub(1);
            waits
        };
        if waits {
            self.barrier.wait();
        }
        7000
    }
}

/// The one model there is.
struct OneModel;

impl Backends for OneModel {
    fn resolve(
        &self,
        _: Capability,
        _: Option<&str>,
        _: &[String],
    ) -> Result<ModelIdentity, Unavailable> {
        Ok(ModelIdentity::new("fake", "fake-filler", "1"))
    }
}

/// Backends that must not be asked: preparing a call that needs a capability asks them.
struct NeverAsked;

impl Backends for NeverAsked {
    fn resolve(
        &self,
        _: Capability,
        _: Option<&str>,
        _: &[String],
    ) -> Result<ModelIdentity, Unavailable> {
        panic!("a call was prepared")
    }
}

type Body = Box<dyn Fn(&PreparedCall, &Context) -> Result<Produced, Failure> + Send + Sync>;

/// `paint`: an action that takes a text, needs a filler, and runs what the test says.
/// It counts its runs.
struct Paint {
    descriptor: ActionDescriptor,
    runs: Arc<AtomicU64>,
    body: Body,
}

impl Action for Paint {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        self.runs.fetch_add(1, Ordering::SeqCst);
        (self.body)(call, context)
    }
}

fn paint_with(
    runs: &Arc<AtomicU64>,
    body: impl Fn(&PreparedCall, &Context) -> Result<Produced, Failure> + Send + Sync + 'static,
) -> Arc<dyn Action> {
    let descriptor = ActionDescriptor::draft("paint", "1.0.0", Kind::Primitive, "A test action.")
        .argument(Argument::required("text", "A text.", ValueType::Text))
        .argument(Argument::optional(
            "fail",
            "Fail, saying this.",
            ValueType::Text,
        ))
        .needs(Requirement::new(Capability::Fill))
        .build()
        .unwrap();
    Arc::new(Paint {
        descriptor,
        runs: runs.clone(),
        body: Box::new(body),
    })
}

fn paint(runs: &Arc<AtomicU64>) -> Arc<dyn Action> {
    paint_with(runs, |call, context| {
        context.diagnostics().write("made.txt", b"paint").unwrap();
        let fail = call.arguments().iter().find(|(name, _)| name == "fail");
        match fail {
            Some((_, Value::Text(message))) => Err(Failure::error("paint", message)),
            _ => Ok(Produced::nothing()),
        }
    })
}

fn call(text: &str) -> Call {
    Call::new("paint").argument("text", Value::text(text))
}

fn id(text: &str) -> RequestId {
    RequestId::parse(text).unwrap()
}

fn asked(text: &str, request: &str) -> Request {
    Request::new(call(text)).with_id(id(request))
}

struct World {
    dir: Scratch,
    fs: Arc<ShimFs>,
    runs: Arc<AtomicU64>,
}

impl World {
    fn new() -> World {
        World::on(ShimFs::new())
    }

    fn on(fs: ShimFs) -> World {
        World {
            dir: Scratch::new("admit"),
            fs: Arc::new(fs),
            runs: Arc::new(AtomicU64::new(0)),
        }
    }

    fn root(&self) -> PathBuf {
        self.dir.path().join("store")
    }

    fn store(&self) -> Arc<Store> {
        Arc::new(Store::open(&self.root(), self.fs.clone()).unwrap())
    }

    fn ran(&self) -> u64 {
        self.runs.load(Ordering::SeqCst)
    }

    /// An executor over this store with `paint` and `echo`, one model, the config of no
    /// bytes, and a clock that says 7000.
    fn engine(&self, store: Arc<Store>) -> Engine {
        engine(store, vec![paint(&self.runs)])
    }
}

fn engine(store: Arc<Store>, actions: Vec<Arc<dyn Action>>) -> Engine {
    let mut registry = Registry::new();
    registry.register(Arc::new(Echo::new())).unwrap();
    for action in actions {
        registry.register(action).unwrap();
    }
    Engine::over(registry, Arc::new(OneModel), store, Digest::of_bytes(b""))
        .with_clock(Arc::new(Stopped(7000)))
}

/// What a store holds: its generation's number, each name outside runs with its
/// content, and each run with its names.
type Held = (u64, Vec<(String, Vec<u8>)>, Vec<(String, Vec<String>)>);

fn held(root: &Path, fs: Arc<dyn Fs>) -> Held {
    let reader = Reader::open(root, fs).unwrap();
    let generation = reader.snapshot().unwrap();
    let names = generation.entries().iter();
    let runs = generation.runs().iter();
    (
        generation.number(),
        names
            .map(|(name, entry)| {
                assert_eq!(entry.kind, store::Kind::Diagnostic, "{name}");
                (name.clone(), reader.read(entry).unwrap())
            })
            .collect(),
        runs.map(|(id, run)| (id.clone(), run.entries().keys().cloned().collect()))
            .collect(),
    )
}

fn ran(answer: Result<Answer, CallError>) -> whizbang_core::contract::Finished {
    match answer {
        Ok(Answer::Ran(finished)) => finished,
        other => panic!("the request did not run: {other:?}"),
    }
}

fn repeat(answer: Result<Answer, CallError>) -> whizbang_core::contract::Earlier {
    match answer {
        Ok(Answer::Repeat(earlier)) => earlier,
        other => panic!("not answered as a repeat: {other:?}"),
    }
}

fn completed() -> Outcome {
    Outcome::Completed {
        outputs: Vec::new(),
    }
}

// --- the id ------------------------------------------------------------------------------------

#[test]
fn a_request_id_is_bounded_text_in_a_plain_alphabet() {
    for good in [
        "a",
        "0",
        "7f1c9a52-3b7e-4f0a-9c1d-2e5b8a6d4c3f",
        "01JCQ7ZK3M5X8V2N4B6D9F0GHT",
        "Order_17.retry:2",
        &"x".repeat(REQUEST_ID_LIMIT),
    ] {
        assert_eq!(RequestId::parse(good).unwrap().as_str(), good);
    }
    // Letter case counts.
    assert_ne!(id("A"), id("a"));
    for bad in [
        "",
        " ",
        "a b",
        "a/b",
        "../a",
        "a\\b",
        "a\nb",
        "a\u{1b}[31m",
        "caf\u{e9}",
        "a\"b",
        "a\0",
        &"x".repeat(REQUEST_ID_LIMIT + 1),
    ] {
        let why = RequestId::parse(bad).unwrap_err();
        assert!(why.contains("is no request id"), "{why}");
        // What was refused is shown escaped, and cut: no control character reaches a
        // terminal, and the refusal is not as long as what it refuses.
        assert!(!why.chars().any(char::is_control), "{why:?}");
        assert!(why.len() < 300, "{}", why.len());
    }
    let long = RequestId::parse(&"\u{7}".repeat(100_000)).unwrap_err();
    assert!(long.contains("(100000 characters)"), "{long}");
    assert!(long.len() < 800, "{}", long.len());
    // The names an id is filed under are its own and no other id's.
    assert_eq!(id("a.b").admission_name(), "requests/a.b/admitted");
    assert_eq!(id("a.b").ending_name(), "requests/a.b/ended");
}

// --- the fingerprint of a request ----------------------------------------------------------------

#[test]
fn the_fingerprint_of_a_request_is_of_the_call_as_it_was_given() {
    // Two fixed vectors, worked out from the words of ADR 0007 and ADR 0001 by a script
    // apart from this code, which is not kept in the repository.
    let simple = Call::new("echo").argument("text", Value::text("hello"));
    assert_eq!(
        request_fingerprint(&simple).hex(),
        "f93fc011cfbdeda221670f83fa218719c8133d99d92b95492d70211584063c40"
    );
    let artifact = |bytes: &[u8]| ArtifactId(Digest::of_bytes(bytes));
    let full = Call::new("echo")
        .argument("text", Value::text("hello"))
        .argument("ratio", Value::Number(1.5))
        .argument("loud", Value::Boolean(true))
        .argument("count", Value::Integer(-2))
        .input("mask", artifact(b"three"))
        .input("image", artifact(b"one"))
        .input("image", artifact(b"two"));
    let fingerprint = request_fingerprint(&full);
    assert_eq!(
        fingerprint.hex(),
        "8a5a17a7d500fd9fbbdfa6bed35e185ff40ec8051700579375951f705b46e899"
    );
    assert_eq!(Request::new(full.clone()).fingerprint(), fingerprint);
    // The id is not part of it.
    assert_eq!(
        Request::new(full.clone()).with_id(id("a")).fingerprint(),
        fingerprint
    );

    // Negative zero is zero, as in a prepared call.
    let zero = |number: f64| Call::new("echo").argument("ratio", Value::Number(number));
    assert_eq!(
        request_fingerprint(&zero(0.0)),
        request_fingerprint(&zero(-0.0))
    );
    // Each of these is another request.
    let others = [
        Call::new("echo2").argument("text", Value::text("hello")),
        Call::new("echo").argument("text", Value::text("hello ")),
        Call::new("echo").argument("txt", Value::text("hello")),
        Call::new("echo"),
        simple.clone().argument("count", Value::Integer(1)),
        simple.clone().argument("count", Value::Number(1.0)),
        simple.clone().input("image", artifact(b"one")),
        simple.clone().input("mask", artifact(b"one")),
        simple
            .clone()
            .input("image", artifact(b"one"))
            .input("image", artifact(b"two")),
        simple
            .clone()
            .input("image", artifact(b"two"))
            .input("image", artifact(b"one")),
    ];
    let mut seen = std::collections::BTreeSet::from([request_fingerprint(&simple)]);
    for other in &others {
        assert!(seen.insert(request_fingerprint(other)), "{other:?}");
    }
}

// --- the two records -----------------------------------------------------------------------------

const ADMISSION: &str = concat!(
    r#"{"record":"admission/1","run":"7000-1","request":"order-17","#,
    r#""fingerprint":"sha256:f93fc011cfbdeda221670f83fa218719c8133d99d92b95492d70211584063c40","#,
    r#""prepared":"sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","#,
    r#""action":"echo","version":"1.0.0","start_ms":7000}"#,
    "\n"
);

const ENDING: &str = concat!(
    r#"{"record":"ending/1","run":"7000-1","request":"order-17","status":"completed","#,
    r#""end_ms":7001}"#,
    "\n"
);

#[test]
fn an_admission_and_an_ending_are_written_one_way_and_read_back() {
    let admission = Admission::from_bytes(ADMISSION.as_bytes()).unwrap();
    assert_eq!(admission.run().as_str(), "7000-1");
    assert_eq!(admission.request(), Some(&id("order-17")));
    assert_eq!(
        admission.fingerprint(),
        request_fingerprint(&Call::new("echo").argument("text", Value::text("hello")))
    );
    assert_eq!(admission.prepared(), Digest::of_bytes(b""));
    assert_eq!((admission.action(), admission.version()), ("echo", "1.0.0"));
    assert_eq!(admission.start_ms(), 7000);
    assert_eq!(admission.to_bytes(), ADMISSION.as_bytes());
    // A run that no id was given for.
    let without = ADMISSION.replace(r#""request":"order-17""#, r#""request":null"#);
    let admission = Admission::from_bytes(without.as_bytes()).unwrap();
    assert_eq!(admission.request(), None);
    assert_eq!(admission.to_bytes(), without.as_bytes());

    let ending = Ending::from_bytes(ENDING.as_bytes()).unwrap();
    assert_eq!(ending.run().as_str(), "7000-1");
    assert_eq!(ending.request(), &id("order-17"));
    assert_eq!(
        (ending.status(), ending.end_ms()),
        (Status::Completed, 7001)
    );
    assert_eq!(ending.to_bytes(), ENDING.as_bytes());
}

#[test]
fn what_is_not_an_admission_or_an_ending_is_refused_without_being_trusted() {
    type Fault = fn(&RequestRecordError) -> bool;
    let malformed: Fault = |e| matches!(e, RequestRecordError::Malformed(_));
    let inconsistent: Fault = |e| matches!(e, RequestRecordError::Inconsistent(_));
    let other_version: Fault = |e| matches!(e, RequestRecordError::OtherVersion { .. });
    let not_canonical: Fault = |e| matches!(e, RequestRecordError::NotCanonical);
    let too_large: Fault = |e| matches!(e, RequestRecordError::TooLarge { .. });
    let long = "x".repeat(2000);
    // An escape character, thirty times, as JSON writes it inside a string.
    let control = r"\u001b[31m".repeat(30);
    let cases: Vec<(&str, String, Fault)> = vec![
        ("no bytes", String::new(), malformed),
        ("not JSON", "not json\n".to_owned(), malformed),
        ("an empty object", "{}\n".to_owned(), malformed),
        (
            "another version",
            ADMISSION.replace("admission/1", "admission/2"),
            other_version,
        ),
        (
            "another kind",
            ADMISSION.replace("admission/1", &control),
            other_version,
        ),
        (
            "an unknown field",
            ADMISSION.replace(r#""start_ms""#, r#""host":"x","start_ms""#),
            malformed,
        ),
        (
            "a field left out",
            ADMISSION.replace(r#""request":"order-17","#, ""),
            malformed,
        ),
        (
            "a field twice",
            ADMISSION.replace(r#""run":"7000-1","#, r#""run":"7000-1","run":"7000-2","#),
            malformed,
        ),
        (
            "a run that is no run id",
            ADMISSION.replace("7000-1", "../7000"),
            inconsistent,
        ),
        (
            "a request that is no request id",
            ADMISSION.replace("order-17", "order 17"),
            inconsistent,
        ),
        (
            "an empty request",
            ADMISSION.replace("order-17", ""),
            inconsistent,
        ),
        (
            "an action that is no action's name",
            ADMISSION.replace(r#""action":"echo""#, r#""action":"Echo it""#),
            inconsistent,
        ),
        (
            "a version that is none",
            ADMISSION.replace("1.0.0", "one"),
            inconsistent,
        ),
        (
            "a digest that is none",
            ADMISSION.replace("sha256:f93f", "sha256:zzzz"),
            malformed,
        ),
        (
            "a time under zero",
            ADMISSION.replace("7000}", "-1}"),
            malformed,
        ),
        (
            "a space between tokens",
            ADMISSION.replace(r#","run""#, r#", "run""#),
            not_canonical,
        ),
        ("no newline", ADMISSION.trim_end().to_owned(), not_canonical),
        (
            "something after the record",
            format!("{ADMISSION}{ADMISSION}"),
            malformed,
        ),
        (
            "more bytes than a record may be",
            ADMISSION.replace("order-17", &long),
            too_large,
        ),
    ];
    for (what, bytes, fault) in &cases {
        let error = Admission::from_bytes(bytes.as_bytes()).unwrap_err();
        assert!(fault(&error), "{what}: {error:?}");
        // Whatever the bytes held, the sentence holds no control character and is
        // short.
        let said = error.to_string();
        assert!(!said.chars().any(char::is_control), "{what}: {said:?}");
        assert!(said.len() < 500, "{what}: {}", said.len());
    }
    // A record of exactly the most bytes is looked at; one byte more is not.
    assert!(ADMISSION.len() as u64 <= REQUEST_RECORD_MAX_BYTES);
    let most = vec![b' '; REQUEST_RECORD_MAX_BYTES as usize];
    assert!(malformed(&Admission::from_bytes(&most).unwrap_err()));
    let over = vec![b' '; REQUEST_RECORD_MAX_BYTES as usize + 1];
    assert!(too_large(&Admission::from_bytes(&over).unwrap_err()));

    // An ending is read as strictly; and neither record is read as the other.
    let endings: Vec<(&str, String, Fault)> = vec![
        ("an admission", ADMISSION.to_owned(), other_version),
        (
            "an unknown field",
            ENDING.replace(r#""end_ms""#, r#""why":"x","end_ms""#),
            malformed,
        ),
        (
            "a status that is none",
            ENDING.replace("completed", "running"),
            malformed,
        ),
        (
            "no request",
            ENDING.replace(r#""request":"order-17""#, r#""request":null"#),
            malformed,
        ),
        (
            "a run that is no run id",
            ENDING.replace("7000-1", "Run 1"),
            inconsistent,
        ),
        (
            "a request that is no request id",
            ENDING.replace("order-17", "a/b"),
            inconsistent,
        ),
        ("no newline", ENDING.trim_end().to_owned(), not_canonical),
        (
            "more bytes than a record may be",
            ENDING.replace("order-17", &long),
            too_large,
        ),
    ];
    for (what, bytes, fault) in &endings {
        let error = Ending::from_bytes(bytes.as_bytes()).unwrap_err();
        assert!(fault(&error), "{what}: {error:?}");
        assert!(!error.to_string().chars().any(char::is_control), "{what}");
    }
    assert!(other_version(
        &Admission::from_bytes(ENDING.as_bytes()).unwrap_err()
    ));
}

// --- a request with an id ------------------------------------------------------------------------

#[test]
fn a_request_with_an_id_is_admitted_with_its_run_in_one_step_and_ended_in_another() {
    let world = World::new();
    let store = world.store();
    // What the action sees of the store while it runs: the run exists, admitted, and
    // the id's record is there with it, in one generation.
    let seen = Arc::new(Mutex::new(None));
    let looking = {
        let (root, fs, seen) = (world.root(), world.fs.clone(), seen.clone());
        paint_with(&world.runs, move |_, _| {
            *seen.lock().unwrap() = Some(held(&root, fs.clone()));
            Ok(Produced::nothing())
        })
    };
    let engine = engine(store, vec![looking]);
    let finished = ran(engine.request(&asked("hello", "order-17")));
    let run = finished.run().as_str().to_owned();
    assert_eq!(finished.outcome(), &completed());

    let (number, names, runs) = seen.lock().unwrap().clone().unwrap();
    assert_eq!(number, 1);
    assert_eq!(runs, [(run.clone(), vec![ADMITTED_NAME.to_owned()])]);
    assert_eq!(names.len(), 1);
    assert_eq!(names[0].0, "requests/order-17/admitted");
    let admission = Admission::from_bytes(&names[0].1).unwrap();
    assert_eq!(admission.run().as_str(), run);
    assert_eq!(admission.request(), Some(&id("order-17")));
    assert_eq!(admission.fingerprint(), request_fingerprint(&call("hello")));
    assert_eq!(
        admission.prepared(),
        engine.prepare(&call("hello")).unwrap().fingerprint()
    );
    assert_eq!(
        (admission.action(), admission.version()),
        ("paint", "1.0.0")
    );
    assert_eq!(admission.start_ms(), 7000);

    // After the end: one generation more, the run's record with the id in it, and the
    // id's ending outside runs.
    let (number, names, runs) = held(&world.root(), world.fs.clone());
    assert_eq!(number, 2);
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].1, ["admitted", "journal", "record"]);
    let names: Vec<(&str, &[u8])> = names.iter().map(|(n, b)| (n.as_str(), &b[..])).collect();
    assert_eq!(names.len(), 2);
    assert_eq!(
        names[0],
        ("requests/order-17/admitted", &admission.to_bytes()[..])
    );
    assert_eq!(names[1].0, "requests/order-17/ended");
    let ending = Ending::from_bytes(names[1].1).unwrap();
    assert_eq!(ending.run().as_str(), run);
    assert_eq!(ending.request(), &id("order-17"));
    assert_eq!(
        (ending.status(), ending.end_ms()),
        (Status::Completed, 7000)
    );

    let reader = Reader::open(&world.root(), world.fs.clone()).unwrap();
    let generation = reader.snapshot().unwrap();
    let record = record::stored(&generation, &run, |e| reader.read(e))
        .unwrap()
        .unwrap();
    assert_eq!(record.identity().request.as_deref(), Some("order-17"));
    // The run's own admission is the same record, under the run's names.
    let own = admitted(&generation, &run, |e| reader.read(e))
        .unwrap()
        .unwrap();
    assert_eq!(own, admission);
}

#[test]
fn a_run_that_no_id_was_given_for_has_its_admission_and_no_record_outside_runs() {
    let world = World::new();
    let engine = world.engine(world.store());
    let finished = engine.submit(&call("hello")).unwrap();
    // The same through the entry point that takes a request.
    let second = ran(engine.request(&Request::new(call("hello"))));
    assert_ne!(finished.run(), second.run());
    let (_, names, runs) = held(&world.root(), world.fs.clone());
    assert_eq!(names, []);
    assert_eq!(runs.len(), 2);
    let reader = Reader::open(&world.root(), world.fs.clone()).unwrap();
    let generation = reader.snapshot().unwrap();
    for run in [finished.run(), second.run()] {
        let admission = admitted(&generation, run.as_str(), |e| reader.read(e));
        let admission = admission.unwrap().unwrap();
        assert_eq!(admission.request(), None);
        assert_eq!(admission.fingerprint(), request_fingerprint(&call("hello")));
        let record = record::stored(&generation, run.as_str(), |e| reader.read(e));
        assert_eq!(record.unwrap().unwrap().identity().request, None);
    }
    // A run that other hands made has no admission, and one that names another run is
    // not this run's.
    let store = world.store_after(engine);
    let mut publication = store.begin().unwrap();
    publication
        .put_in("theirs", "notes", store::Kind::Diagnostic, b"x")
        .unwrap();
    publication
        .put_in(
            "7000-1",
            ADMITTED_NAME,
            store::Kind::Diagnostic,
            ADMISSION.as_bytes(),
        )
        .unwrap();
    publication
        .put_in(
            "9",
            ADMITTED_NAME,
            store::Kind::Diagnostic,
            ADMISSION.as_bytes(),
        )
        .unwrap();
    publication.publish().unwrap();
    let generation = store.snapshot().unwrap();
    let of = |run: &str| admitted(&generation, run, |e| store.read(e));
    assert!(of("nowhere").is_none());
    assert!(matches!(
        of("theirs"),
        Some(Err(StoredAdmissionError::NoAdmission))
    ));
    assert!(matches!(of("9"), Some(Err(StoredAdmissionError::OtherRun))));
    assert_eq!(of("7000-1").unwrap().unwrap().run().as_str(), "7000-1");
}

impl World {
    /// The store again, once this executor, which held it, is gone.
    fn store_after(&self, engine: Engine) -> Arc<Store> {
        drop(engine);
        self.store()
    }
}

// --- the same request again ----------------------------------------------------------------------

/// An executor as after a change of everything a preparation depends on: another
/// config, backends that panic when a call is prepared, and no action registered at
/// all.
fn changed(store: Arc<Store>) -> Engine {
    Engine::over(
        Registry::new(),
        Arc::new(NeverAsked),
        store,
        Digest::of_bytes(b"another config"),
    )
    .with_clock(Arc::new(Stopped(9000)))
}

#[test]
fn the_same_request_again_after_the_config_changed_returns_the_first_run_and_prepares_nothing() {
    let world = World::new();
    let store = world.store();
    let engine = world.engine(store.clone());
    // The test's prepare panics when it is reached, on this call.
    let panics = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        engine_with(store.clone(), &world.runs, Arc::new(NeverAsked)).prepare(&call("hello"))
    }));
    assert!(panics.is_err());

    let finished = ran(engine.request(&asked("hello", "order-17")));
    let failed = ran(engine.request(
        &Request::new(call("hello").argument("fail", Value::text("no paint left")))
            .with_id(id("order-18")),
    ));
    assert_eq!(world.ran(), 2);
    let before = held(&world.root(), world.fs.clone());
    let events = world.fs.events().len();

    // The config changed, the backends changed, and the action is gone. The same
    // request under the same id is answered with the run it made, as it ended.
    let later = changed(store.clone());
    let earlier = repeat(later.request(&asked("hello", "order-17")));
    assert_eq!(earlier.run(), finished.run());
    assert_eq!(earlier.start_ms(), 7000);
    assert_eq!(earlier.state(), &RunState::Ended(completed()));
    // A run that failed is answered with its failure, and is not tried again.
    let again = Request::new(call("hello").argument("fail", Value::text("no paint left")))
        .with_id(id("order-18"));
    let earlier = repeat(later.request(&again));
    assert_eq!(earlier.run(), failed.run());
    assert_eq!(earlier.state(), &RunState::Ended(failed.outcome().clone()));
    assert!(matches!(failed.outcome(), Outcome::Error { .. }));
    // And by the executor that ran it, the same.
    assert_eq!(
        repeat(engine.request(&asked("hello", "order-17"))).run(),
        finished.run()
    );

    // Nothing was dispatched and nothing was written: no action ran, and the store is
    // as it was, with no call made that changes it.
    assert_eq!(world.ran(), 2);
    assert_eq!(held(&world.root(), world.fs.clone()), before);
    assert_eq!(world.fs.events().len(), events);

    // Without the id the same executor does reach prepare, and refuses the call: the
    // repeat was answered by the id alone.
    assert!(matches!(
        later.request(&Request::new(call("hello"))),
        Err(CallError::Refused(Refusal::UnknownAction { .. }))
    ));
}

fn engine_with(store: Arc<Store>, runs: &Arc<AtomicU64>, backends: Arc<dyn Backends>) -> Engine {
    let mut registry = Registry::new();
    registry.register(paint(runs)).unwrap();
    Engine::over(registry, backends, store, Digest::of_bytes(b""))
        .with_clock(Arc::new(Stopped(7000)))
}

#[test]
fn a_different_request_under_the_same_id_is_a_conflict() {
    let world = World::new();
    let store = world.store();
    let engine = world.engine(store.clone());
    let finished = ran(engine.request(&asked("hello", "order-17")));
    let before = held(&world.root(), world.fs.clone());

    let refused = engine.request(&asked("goodbye", "order-17")).unwrap_err();
    assert_eq!(
        refused,
        CallError::Refused(Refusal::Conflict {
            request: id("order-17"),
            run: finished.run().clone(),
        })
    );
    assert_eq!(
        refused.to_string(),
        format!(
            "the request id order-17 was used for another request, which made the run {}: \
             an id is one request's, and this one was not run",
            finished.run()
        )
    );
    // Nothing was prepared to say so: an executor that cannot prepare says the same.
    assert_eq!(
        changed(store.clone())
            .request(&asked("goodbye", "order-17"))
            .unwrap_err(),
        refused
    );
    // Another action, and the same call with an input, are other requests too.
    let other_action = Request::new(Call::new("echo").argument("text", Value::text("hello")));
    assert!(matches!(
        engine.request(&other_action.with_id(id("order-17"))),
        Err(CallError::Refused(Refusal::Conflict { .. }))
    ));
    assert_eq!(world.ran(), 1);
    assert_eq!(held(&world.root(), world.fs.clone()), before);
    // The id is still the first request's.
    assert_eq!(
        repeat(engine.request(&asked("hello", "order-17"))).run(),
        finished.run()
    );
}

#[test]
fn known_a_default_given_by_hand_is_another_request_than_the_default_left_out() {
    // The limit of "the same request", pinned: the fingerprint of a request is of the
    // call as it was given, before any descriptor is read. A default given by hand and
    // the same default left out are one preparation (ADR 0004) and two requests, so the
    // second under the first's id is a conflict. So is a backend named and the same
    // backend left to the default (ADR 0007).
    let toned = ActionDescriptor::draft("tone", "1.0.0", Kind::Primitive, "A test action.")
        .argument(Argument::with_default(
            "tone",
            "A tone.",
            ValueType::Text,
            Value::text("plain"),
        ))
        .build()
        .unwrap();
    struct Toned(ActionDescriptor);
    impl Action for Toned {
        fn descriptor(&self) -> &ActionDescriptor {
            &self.0
        }
        fn run(&self, _: &PreparedCall, _: &Context) -> Result<Produced, Failure> {
            Ok(Produced::nothing())
        }
    }
    let world = World::new();
    let engine = engine(world.store(), vec![Arc::new(Toned(toned))]);
    let left_out = Call::new("tone");
    let by_hand = Call::new("tone").argument("tone", Value::text("plain"));
    assert_eq!(
        engine.prepare(&left_out).unwrap().fingerprint(),
        engine.prepare(&by_hand).unwrap().fingerprint()
    );
    assert_ne!(
        request_fingerprint(&left_out),
        request_fingerprint(&by_hand)
    );
    ran(engine.request(&Request::new(left_out).with_id(id("a"))));
    assert!(matches!(
        engine.request(&Request::new(by_hand).with_id(id("a"))),
        Err(CallError::Refused(Refusal::Conflict { .. }))
    ));
}

#[test]
fn a_repeat_after_a_restart_returns_the_first_run() {
    let world = World::new();
    let engine = world.engine(world.store());
    let finished = ran(engine.request(&asked("hello", "order-17")));
    let failing = Request::new(call("hello").argument("fail", Value::text("no paint left")))
        .with_id(id("order-18"));
    let failed = ran(engine.request(&failing));
    // Power is lost once both were reported, and the process is gone.
    world.fs.crash(Lost::Unsynced);
    drop(engine);

    let store = Arc::new(Store::open(&world.root(), Arc::new(RealFs)).unwrap());
    let engine = world.engine(store);
    let earlier = repeat(engine.request(&asked("hello", "order-17")));
    assert_eq!(earlier.run(), finished.run());
    assert_eq!(earlier.state(), &RunState::Ended(completed()));
    let earlier = repeat(engine.request(&failing));
    assert_eq!(earlier.run(), failed.run());
    assert_eq!(earlier.state(), &RunState::Ended(failed.outcome().clone()));
    assert!(matches!(
        engine.request(&asked("other", "order-17")),
        Err(CallError::Refused(Refusal::Conflict { .. }))
    ));
    assert_eq!(world.ran(), 2);
    // An executor that only reads the store answers the same, and runs nothing new.
    let reader = Arc::new(Reader::open(&world.root(), Arc::new(RealFs)).unwrap());
    let mut registry = Registry::new();
    registry.register(paint(&world.runs)).unwrap();
    let reading = Engine::new(registry, Arc::new(OneModel), reader, Digest::of_bytes(b""));
    let earlier = repeat(reading.request(&asked("hello", "order-17")));
    assert_eq!(earlier.state(), &RunState::Ended(completed()));
    assert_eq!(
        reading.request(&asked("hello", "order-19")).unwrap_err(),
        CallError::Refused(Refusal::ReadOnly)
    );
    assert_eq!(world.ran(), 2);
}

#[test]
fn a_repeat_after_the_run_was_pruned_is_answered_from_the_record_and_dispatches_nothing() {
    let world = World::new();
    let store = world.store();
    let engine = world.engine(store.clone());
    let finished = ran(engine.request(&asked("hello", "order-17")));
    let failing = Request::new(call("hello").argument("fail", Value::text("no paint left")))
        .with_id(id("order-18"));
    let failed = ran(engine.request(&failing));
    let (_, records, _) = held(&world.root(), world.fs.clone());
    assert_eq!(records.len(), 4);

    // Every run is deleted and the store is pruned: the runs, their records, their
    // journals and their diagnostics are gone, from the generation and from the disk.
    store
        .delete(&[finished.run().as_str(), failed.run().as_str()])
        .unwrap();
    let pruned = store.prune().unwrap();
    assert!(pruned.objects_removed >= 4, "{pruned:?}");
    let (_, after, runs) = held(&world.root(), world.fs.clone());
    assert_eq!(runs, []);
    // The four records of the two ids are as they were, and are read back.
    assert_eq!(after, records);

    let events = world.fs.events().len();
    for engine in [&engine, &changed(store.clone())] {
        let earlier = repeat(engine.request(&asked("hello", "order-17")));
        assert_eq!(earlier.run(), finished.run());
        assert_eq!(earlier.start_ms(), 7000);
        assert_eq!(
            earlier.state(),
            &RunState::Removed {
                ended: Some(Status::Completed)
            }
        );
        let earlier = repeat(engine.request(&failing));
        assert_eq!(earlier.run(), failed.run());
        assert_eq!(
            earlier.state(),
            &RunState::Removed {
                ended: Some(Status::Error)
            }
        );
        // The id is not free again: another request under it is still a conflict.
        assert!(matches!(
            engine.request(&asked("other", "order-17")),
            Err(CallError::Refused(Refusal::Conflict { .. }))
        ));
    }
    assert_eq!(world.ran(), 2);
    assert_eq!(world.fs.events().len(), events);
    assert_eq!(held(&world.root(), world.fs.clone()).1, records);
}

// --- one id, one run -----------------------------------------------------------------------------

#[test]
fn two_simultaneous_requests_with_one_id_produce_one_run() {
    // Two executors over one store, and a clock that does not move and holds each
    // request, once it was looked up and prepared, until the other is there too: both
    // found the id free. The store admits one. The other is answered with the run of
    // the first, which is still executing: its action waits until the other request
    // has its answer.
    let world = World::new();
    let store = world.store();
    let clock = Arc::new(Together::new(2));
    let (answered, waiting) = mpsc::channel::<()>();
    let waiting = Arc::new(Mutex::new(waiting));
    let engines: Vec<Engine> = (0..2)
        .map(|_| {
            let waiting = waiting.clone();
            let held = paint_with(&world.runs, move |_, _| {
                waiting.lock().unwrap().recv().unwrap();
                Ok(Produced::nothing())
            });
            engine(store.clone(), vec![held]).with_clock(clock.clone())
        })
        .collect();
    let answers: Vec<Answer> = std::thread::scope(|scope| {
        let threads: Vec<_> = engines
            .iter()
            .map(|engine| {
                let answered = answered.clone();
                scope.spawn(move || {
                    let answer = engine.request(&asked("hello", "order-17")).unwrap();
                    answered.send(()).unwrap();
                    answer
                })
            })
            .collect();
        threads.into_iter().map(|t| t.join().unwrap()).collect()
    });
    let finished: Vec<_> = answers
        .iter()
        .filter_map(|a| match a {
            Answer::Ran(finished) => Some(finished),
            Answer::Repeat(_) => None,
        })
        .collect();
    let earlier: Vec<_> = answers
        .iter()
        .filter_map(|a| match a {
            Answer::Repeat(earlier) => Some(earlier),
            Answer::Ran(_) => None,
        })
        .collect();
    assert_eq!((finished.len(), earlier.len()), (1, 1), "{answers:?}");
    assert_eq!(earlier[0].run(), finished[0].run());
    // The first run was admitted by this writer and had not ended.
    assert_eq!(earlier[0].state(), &RunState::NotEnded { here: true });
    assert_eq!(world.ran(), 1);
    let (_, names, runs) = held(&world.root(), world.fs.clone());
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!(runs[0].0, finished[0].run().as_str());
    assert_eq!(names.len(), 2);
}

#[test]
fn two_simultaneous_requests_that_differ_under_one_id_are_one_run_and_one_conflict() {
    let world = World::new();
    let store = world.store();
    let clock = Arc::new(Together::new(2));
    let engines: Vec<Engine> = (0..2)
        .map(|_| world.engine(store.clone()).with_clock(clock.clone()))
        .collect();
    let answers: Vec<Result<Answer, CallError>> = std::thread::scope(|scope| {
        let threads: Vec<_> = engines
            .iter()
            .zip(["hello", "goodbye"])
            .map(|(engine, text)| scope.spawn(move || engine.request(&asked(text, "order-17"))))
            .collect();
        threads.into_iter().map(|t| t.join().unwrap()).collect()
    });
    let ran: Vec<_> = answers.iter().filter_map(|a| a.as_ref().ok()).collect();
    assert_eq!(ran.len(), 1, "{answers:?}");
    let refused: Vec<_> = answers.iter().filter_map(|a| a.as_ref().err()).collect();
    assert_eq!(
        refused,
        [&CallError::Refused(Refusal::Conflict {
            request: id("order-17"),
            run: ran[0].run().clone(),
        })]
    );
    assert_eq!(world.ran(), 1);
    assert_eq!(held(&world.root(), world.fs.clone()).2.len(), 1);
}

#[test]
fn many_requests_at_once_under_a_few_ids_make_one_run_for_each_id() {
    // Eight threads, two executors, one store, a clock that does not move: each thread
    // sends each of five ids. Every id has one run, and every answer names it.
    let world = World::new();
    let store = world.store();
    let engines = [world.engine(store.clone()), world.engine(store.clone())];
    let answers: Vec<(usize, Answer)> = std::thread::scope(|scope| {
        let threads: Vec<_> = (0..8)
            .map(|thread| {
                let engine = &engines[thread % 2];
                scope.spawn(move || {
                    (0..5)
                        .map(|n| {
                            let request = asked(&format!("text {n}"), &format!("id-{n}"));
                            (n, engine.request(&request).unwrap())
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        threads
            .into_iter()
            .flat_map(|t| t.join().unwrap())
            .collect()
    });
    assert_eq!(answers.len(), 40);
    for n in 0..5 {
        let of_id: Vec<&Answer> = answers
            .iter()
            .filter(|(m, _)| *m == n)
            .map(|(_, a)| a)
            .collect();
        let made = of_id.iter().filter(|a| matches!(a, Answer::Ran(_))).count();
        assert_eq!(made, 1, "id-{n}");
        assert!(of_id.iter().all(|a| a.run() == of_id[0].run()), "id-{n}");
    }
    assert_eq!(world.ran(), 5);
    assert_eq!(held(&world.root(), world.fs.clone()).2.len(), 5);
}

// --- a run that did not end ----------------------------------------------------------------------

#[test]
fn a_run_whose_process_died_is_answered_as_not_ended_and_is_not_run_again() {
    let world = World::new();
    let store = world.store();
    // Power is lost while the action runs: the run was admitted, durably, and nothing
    // of its end is written.
    let dying = {
        let fs = world.fs.clone();
        paint_with(&world.runs, move |_, _| {
            fs.crash(Lost::Unsynced);
            Ok(Produced::nothing())
        })
    };
    let engine = engine(store, vec![dying]);
    assert!(matches!(
        engine.request(&asked("hello", "order-17")),
        Err(CallError::Store(_))
    ));
    drop(engine);

    let fs: Arc<dyn Fs> = Arc::new(RealFs);
    let (number, names, runs) = held(&world.root(), fs.clone());
    assert_eq!(number, 1);
    assert_eq!(names.len(), 1);
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].1, [ADMITTED_NAME]);
    let store = Arc::new(Store::open(&world.root(), fs.clone()).unwrap());
    let engine = world.engine(store.clone());
    let earlier = repeat(engine.request(&asked("hello", "order-17")));
    assert_eq!(earlier.run().as_str(), runs[0].0);
    // Not by this writer: the process that made it is gone, and it will never end.
    assert_eq!(earlier.state(), &RunState::NotEnded { here: false });
    assert_eq!(world.ran(), 1);
    assert_eq!(held(&world.root(), fs.clone()).0, 1);

    // Deleted and pruned, it is answered as removed before it ended.
    store.delete(&[runs[0].0.as_str()]).unwrap();
    store.prune().unwrap();
    let earlier = repeat(engine.request(&asked("hello", "order-17")));
    assert_eq!(earlier.state(), &RunState::Removed { ended: None });
    assert_eq!(world.ran(), 1);
}

#[test]
fn a_run_whose_end_could_not_be_written_is_answered_as_not_ended_here() {
    // The store refuses every write once the action has started. The caller is told of
    // the store's failure; the run stays as it was admitted, and the same request
    // again is answered with it, and is not run a second time.
    let world = World::new();
    let store = world.store();
    let failing = {
        let fs = world.fs.clone();
        paint_with(&world.runs, move |_, _| {
            fs.refuse_matching(0, |_| true);
            Ok(Produced::nothing())
        })
    };
    let engine = engine(store, vec![failing]);
    assert!(matches!(
        engine.request(&asked("hello", "order-17")),
        Err(CallError::Store(_))
    ));
    let earlier = repeat(engine.request(&asked("hello", "order-17")));
    assert_eq!(earlier.state(), &RunState::NotEnded { here: true });
    assert_eq!(world.ran(), 1);
}

// --- a crash inside the admission step -----------------------------------------------------------

#[test]
fn a_crash_inside_the_admission_step_leaves_no_record_or_all_of_it() {
    // The calls that change the disk during one admission, found by running one: from
    // the request to the moment the action starts.
    let (opened, steps) = {
        let world = World::new();
        let store = world.store();
        let opened = world.fs.events().len();
        let started = Arc::new(AtomicU64::new(0));
        let noting = {
            let (fs, started) = (world.fs.clone(), started.clone());
            paint_with(&world.runs, move |_, _| {
                started.store(fs.events().len() as u64, Ordering::SeqCst);
                Ok(Produced::nothing())
            })
        };
        ran(engine(store, vec![noting]).request(&asked("hello", "order-17")));
        (opened, started.load(Ordering::SeqCst) as usize - opened)
    };
    assert!(steps > 8, "{steps}");

    // Power is lost before each of them in turn, with each way a disk can lose what
    // was not yet durable. Whatever is left, the id's record and the run are there
    // together or not at all.
    let mut outcomes = std::collections::BTreeSet::new();
    for step in 0..=steps {
        for lost in [
            Lost::Unsynced,
            Lost::ContentOnly,
            Lost::Subset(0b0101),
            Lost::Names(0b1010),
        ] {
            let world = World::on(ShimFs::new().crash_before(opened + step, lost));
            let engine = world.engine(world.store());
            let answer = engine.request(&asked("hello", "order-17"));
            assert!(
                matches!(answer, Err(CallError::Store(_))),
                "step {step}: {answer:?}"
            );
            // The action started only where the admission was whole and durable: the
            // last call of the step clears its staging away, and the admission does
            // not depend on it.
            let admitted_here = world.ran() == 1;
            assert!(!admitted_here || step >= steps - 1, "step {step}");
            drop(engine);

            // The next writer recovers the store, and reads what survived.
            let fs: Arc<dyn Fs> = Arc::new(RealFs);
            let store = Arc::new(Store::open(&world.root(), fs.clone()).unwrap());
            let (_, names, runs) = held(&world.root(), fs.clone());
            match (names.as_slice(), runs.as_slice()) {
                ([], []) => {
                    // Nothing: no action had started, and the same request is admitted
                    // and runs, once.
                    assert!(!admitted_here, "step {step}: an action ran unadmitted");
                    let again = world.engine(store);
                    ran(again.request(&asked("hello", "order-17")));
                    assert_eq!(world.ran(), 1, "step {step}");
                    outcomes.insert("nothing");
                }
                ([(name, bytes)], [(run, held)]) => {
                    // All of it: the record names the run, the run holds the same
                    // admission, and the same request is answered with that run.
                    assert_eq!(name, "requests/order-17/admitted", "step {step}");
                    let admission = Admission::from_bytes(bytes).unwrap();
                    assert_eq!(admission.run().as_str(), run, "step {step}");
                    assert_eq!(held, &[ADMITTED_NAME.to_owned()], "step {step}");
                    let before = world.ran();
                    let again = world.engine(store);
                    let earlier = repeat(again.request(&asked("hello", "order-17")));
                    assert_eq!(world.ran(), before, "step {step}");
                    assert_eq!(earlier.run().as_str(), run);
                    assert_eq!(earlier.state(), &RunState::NotEnded { here: false });
                    outcomes.insert("all of it");
                }
                other => panic!("step {step}, {lost:?}: a part of an admission: {other:?}"),
            }
        }
    }
    assert_eq!(
        outcomes.into_iter().collect::<Vec<_>>(),
        ["all of it", "nothing"]
    );
}

// --- what is stored is not trusted ---------------------------------------------------------------

#[test]
fn a_stored_record_of_a_request_that_is_not_one_is_refused_and_nothing_runs() {
    let world = World::new();
    let store = world.store();
    let engine = world.engine(store.clone());
    let plant = |name: &str, bytes: &[u8]| {
        let mut publication = store.begin().unwrap();
        publication.put(name, bytes).unwrap();
        publication.publish().unwrap();
    };
    let record_refused = |request: &str, says: &str| {
        let error = engine.request(&asked("hello", request)).unwrap_err();
        let CallError::Record { what, why } = &error else {
            panic!("{request}: {error:?}");
        };
        assert!(why.contains(says), "{request}: {why}");
        // Nothing the store held reaches a terminal as it is, a path is not named,
        // and the sentence is not as long as what it refuses.
        let said = format!("{error} {error:?} {what}");
        assert!(!said.contains('\u{1b}'), "{said:?}");
        assert!(!said.contains(world.root().to_str().unwrap()), "{said}");
        assert!(said.len() < 1200, "{}", said.len());
    };
    let escape = "\u{1b}[31m".repeat(20);
    plant("requests/garbage/admitted", escape.as_bytes());
    record_refused("garbage", "expected value");
    plant("requests/big/admitted", &vec![b'x'; 5000]);
    // Its length is looked at in its entry, and its content is not read at all: with
    // the content gone from the disk, the answer is still the length.
    let big = store::Digest::of_bytes(&vec![b'x'; 5000]).hex();
    std::fs::remove_file(world.root().join("objects").join(big)).unwrap();
    record_refused("big", "5000 bytes");
    let written = r"\u001b[31m".repeat(20);
    plant(
        "requests/newer/admitted",
        ADMISSION.replace("admission/1", &written).as_bytes(),
    );
    record_refused("newer", "another kind or version");
    // A sound record, filed under another id than the one it names.
    plant("requests/order-18/admitted", ADMISSION.as_bytes());
    record_refused("order-18", "the record of another request");
    assert_eq!(world.ran(), 0);

    // The record of an id whose run has a record that is not one.
    let finished = ran(engine.request(&asked("hello", "order-20")));
    let mut publication = store.begin().unwrap();
    publication
        .put_in(
            finished.run().as_str(),
            "record",
            store::Kind::Diagnostic,
            escape.as_bytes(),
        )
        .unwrap();
    publication.publish().unwrap();
    record_refused("order-20", "not a run record");
    // And of an id whose run is gone and whose ending is of another run.
    let finished = ran(engine.request(&asked("hello", "order-21")));
    store.delete(&[finished.run().as_str()]).unwrap();
    plant("requests/order-21/ended", ENDING.as_bytes());
    record_refused("order-21", "another request or another run");
    assert_eq!(world.ran(), 2);
}

// --- a secret as an id ---------------------------------------------------------------------------

#[test]
fn a_request_id_that_holds_a_secret_is_refused_and_not_shown() {
    let key = made_up_key("");
    let world = World::new();
    let engine = world
        .engine(world.store())
        .with_secrets(Secrets::new([key.clone()]).unwrap());
    for held in [key.clone(), format!("order:{key}:2")] {
        let request = Request::new(call("hello")).with_id(id(&held));
        let refused = engine.request(&request).unwrap_err();
        assert_eq!(refused, CallError::Refused(Refusal::SecretInRequestId));
        let said = format!("{refused} {refused:?}");
        assert!(!said.contains(&key), "{said}");
    }
    // Nothing was looked up, prepared, run or written.
    assert_eq!(world.ran(), 0);
    assert_eq!(held(&world.root(), world.fs.clone()), (0, vec![], vec![]));
    for (path, bytes) in files(&world.root()) {
        let holds = bytes.windows(key.len()).any(|w| w == key.as_bytes());
        assert!(
            !holds && !path.to_string_lossy().contains(&key),
            "{}",
            path.display()
        );
    }
    // An id that holds none is taken as it is.
    ran(engine.request(&asked("hello", "order-17")));
}

fn files(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
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

#[test]
fn a_request_that_is_refused_or_not_ready_records_no_id() {
    // A refusal happens before any run exists: the id is not taken by it, and the same
    // id is free for the request once it can run.
    let world = World::new();
    let store = world.store();
    let unready = engine_with(store.clone(), &world.runs, Arc::new(NoBackends));
    assert!(matches!(
        unready.request(&asked("hello", "order-17")),
        Err(CallError::Refused(Refusal::NotReady { .. }))
    ));
    let invalid = Request::new(Call::new("paint")).with_id(id("order-17"));
    assert!(matches!(
        unready.request(&invalid),
        Err(CallError::Refused(Refusal::Invalid { .. }))
    ));
    assert_eq!(held(&world.root(), world.fs.clone()), (0, vec![], vec![]));
    ran(world.engine(store).request(&asked("hello", "order-17")));
}
