//! Request ids, and the admission of a run (ADR 0007): an id is looked up before
//! anything is prepared; one publication makes the run and the id's record; the record
//! outlives the run; and the same request sent again is answered with the run it made.
//! And the reservation: the ledger is asked in the admission's step, and what was
//! reserved is published with the run and the id's record or not at all.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier, Mutex, mpsc};

use whizbang_core::artifact::ArtifactId;
use whizbang_core::contract::record::{self, Status};
use whizbang_core::contract::request::{StoredAdmissionError, admitted};
use whizbang_core::contract::{
    ADMITTED_NAME, Action, ActionDescriptor, Admission, Answer, Argument, Backends, Call,
    CallError, Capability, Clock, Context, Declined, Ending, Engine, Estimate, Executor, Failure,
    Kind, Ledger, ModelIdentity, NoBackends, Outcome, PreparedCall, Produced, REQUEST_ID_LIMIT,
    REQUEST_RECORD_MAX_BYTES, Refusal, Registry, Request, RequestId, RequestRecordError,
    Requirement, Reservation, RunState, Secrets, Step, Unavailable, Value, ValueType,
    request_fingerprint, reservations,
};
use whizbang_core::contract::{ConfigSource, InputFault, InputPort, Readiness, Stale};
use whizbang_core::identity::Digest;
use whizbang_core::store::fs::{Fs, RealFs};
use whizbang_core::store::{self, Reader, Store};
use whizbang_testkit::echo::Echo;
use whizbang_testkit::fs_shim::{Event, Lost, ShimFs};
use whizbang_testkit::ledger::FakeLedger;
use whizbang_testkit::scratch::Scratch;

use super::execute::Taker;
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
    r#""reserved_micros":500000,"action":"echo","version":"1.0.0","start_ms":7000}"#,
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
    assert_eq!(admission.reserved_micros(), Some(500_000));
    assert_eq!((admission.action(), admission.version()), ("echo", "1.0.0"));
    assert_eq!(admission.start_ms(), 7000);
    assert_eq!(admission.to_bytes(), ADMISSION.as_bytes());
    // A run that no id was given for.
    let without = ADMISSION.replace(r#""request":"order-17""#, r#""request":null"#);
    let admission = Admission::from_bytes(without.as_bytes()).unwrap();
    assert_eq!(admission.request(), None);
    assert_eq!(admission.to_bytes(), without.as_bytes());
    // A run that was admitted with no ledger: nothing was reserved, which is not a
    // reservation of nothing.
    let unreserved = ADMISSION.replace(r#""reserved_micros":500000"#, r#""reserved_micros":null"#);
    let admission = Admission::from_bytes(unreserved.as_bytes()).unwrap();
    assert_eq!(admission.reserved_micros(), None);
    assert_eq!(admission.to_bytes(), unreserved.as_bytes());
    let nothing = ADMISSION.replace(r#""reserved_micros":500000"#, r#""reserved_micros":0"#);
    let admission = Admission::from_bytes(nothing.as_bytes()).unwrap();
    assert_eq!(admission.reserved_micros(), Some(0));

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
            "no word on what was reserved, as the first parts of the task wrote it",
            ADMISSION.replace(r#""reserved_micros":500000,"#, ""),
            malformed,
        ),
        (
            "an amount under zero",
            ADMISSION.replace(r#""reserved_micros":500000"#, r#""reserved_micros":-1"#),
            malformed,
        ),
        (
            "an amount that is not whole",
            ADMISSION.replace(r#""reserved_micros":500000"#, r#""reserved_micros":0.5"#),
            malformed,
        ),
        (
            "the largest amount there is, which is no amount",
            ADMISSION.replace(":500000", ":18446744073709551615"),
            inconsistent,
        ),
        (
            "an amount as text",
            ADMISSION.replace(
                r#""reserved_micros":500000"#,
                r#""reserved_micros":"500000""#,
            ),
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
    // This executor was given no ledger: nothing was reserved, and the store holds no
    // reservation.
    assert_eq!(admission.reserved_micros(), None);
    let reader = Reader::open(&world.root(), world.fs.clone()).unwrap();
    let generation = reader.snapshot().unwrap();
    assert_eq!(reservations(&generation, |e| reader.read(e)), Ok(vec![]));
    assert!(!generation.entries().keys().any(|n| n.starts_with("spend/")));

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
    let answers: Vec<Answer> = whizbang_testkit::threads::scope(|scope| {
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
            Answer::Repeat(_) | Answer::Started(_) => None,
        })
        .collect();
    let earlier: Vec<_> = answers
        .iter()
        .filter_map(|a| match a {
            Answer::Repeat(earlier) => Some(earlier),
            Answer::Ran(_) | Answer::Started(_) => None,
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
    let answers: Vec<Result<Answer, CallError>> = whizbang_testkit::threads::scope(|scope| {
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
    let answers: Vec<(usize, Answer)> = whizbang_testkit::threads::scope(|scope| {
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
    // Abandoned (ADR 0008, "A run with nothing behind it"): open here, and nothing ends it.
    assert!(
        matches!(earlier.state(), RunState::Abandoned { failure: Some(_) }),
        "{:?}",
        earlier.state()
    );
    assert_eq!(world.ran(), 1);
}

// --- a crash inside the admission step -----------------------------------------------------------

#[test]
fn a_crash_inside_the_admission_step_leaves_no_record_or_all_of_it() {
    // An executor with a ledger admits a call that may cost a quarter of a dollar. The
    // calls that change the disk during one admission are found by running one: from
    // the request to the moment the action starts.
    let admitting = |world: &World, ledger: &Arc<FakeLedger>, store: Arc<Store>| {
        engine_with(store, &world.runs, Priced::new(250_000)).with_ledger(ledger.clone())
    };
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
        let mut registry = Registry::new();
        registry.register(noting).unwrap();
        let engine = Engine::over(registry, Priced::new(250_000), store, Digest::of_bytes(b""))
            .with_ledger(Arc::new(FakeLedger::new()));
        ran(engine.request(&asked("hello", "order-17")));
        (opened, started.load(Ordering::SeqCst) as usize - opened)
    };
    assert!(steps > 8, "{steps}");

    // Power is lost before each of them in turn, with each way a disk can lose what
    // was not yet durable: every name undone, every name kept, and every choice there
    // is of which directories, and of which names, lose theirs. How many directories
    // and names a crash at a step leaves undecided is learnt from the first crash
    // there, and every mask over them is then tried. Whatever is left, the id's
    // record, the run and the reservation are there together or none of them is.
    let mut outcomes = std::collections::BTreeSet::new();
    let (mut most_dirs, mut most_names, mut crashes) = (0, 0, 0);
    for step in 0..=steps {
        let mut ways = vec![Lost::Unsynced, Lost::ContentOnly, Lost::Subset(0)];
        let mut next = 0;
        while next < ways.len() {
            let lost = &ways[next].clone();
            next += 1;
            crashes += 1;
            let world = World::on(ShimFs::new().crash_before(opened + step, *lost));
            let ledger = Arc::new(FakeLedger::new());
            let engine = admitting(&world, &ledger, world.store());
            let answer = engine.request(&asked("hello", "order-17"));
            assert!(
                matches!(answer, Err(CallError::Store(_))),
                "step {step}: {answer:?}"
            );
            if *lost == Lost::Subset(0) {
                let (dirs, names) = (world.fs.crash_choices(), world.fs.crash_names());
                assert!(dirs <= 10 && names <= 10, "step {step}: {dirs} {names}");
                ways.extend((1..1u64 << dirs).map(Lost::Subset));
                ways.extend((0..1u64 << names).map(Lost::Names));
                most_dirs = most_dirs.max(dirs);
                most_names = most_names.max(names);
            }
            // The action started only where the admission was whole and durable: the
            // last call of the step clears its staging away, and the admission does
            // not depend on it.
            let admitted_here = world.ran() == 1;
            assert!(!admitted_here || step >= steps - 1, "step {step}");
            drop(engine);

            // The next writer recovers the store, and reads what survived: whatever
            // is on the disk after a crash is durable.
            let fs: Arc<dyn Fs> = Arc::new(ShimFs::new());
            let store = Arc::new(Store::open(&world.root(), fs.clone()).unwrap());
            let (_, names, runs) = held(&world.root(), fs.clone());
            let reserved = ledger.reservations(store.as_ref()).unwrap();
            let at = format!("step {step}, {lost:?}");
            match (names.as_slice(), runs.as_slice()) {
                ([], []) => {
                    // Nothing: no id's record, no run, no reservation. No action had
                    // started, and the same request is admitted and runs, once.
                    assert_eq!(reserved, [], "{at}");
                    assert!(!admitted_here, "{at}: an action ran unadmitted");
                    let again = admitting(&world, &ledger, store.clone());
                    ran(again.request(&asked("hello", "order-17")));
                    assert_eq!(world.ran(), 1, "{at}");
                    assert_eq!(ledger.reservations(store.as_ref()).unwrap().len(), 1);
                    outcomes.insert("nothing");
                }
                ([(name, bytes), (sequence, _), (reservation, stored)], [(run, held)]) => {
                    // All of it: the id's record names the run, the run holds the same
                    // admission, the reservation is of that run and that request, for
                    // the amount the admission says, and the same request is answered
                    // with that run and reserves nothing more.
                    assert_eq!(name, "requests/order-17/admitted", "{at}");
                    let admission = Admission::from_bytes(bytes).unwrap();
                    assert_eq!(admission.run().as_str(), run, "{at}");
                    assert_eq!(admission.reserved_micros(), Some(250_000), "{at}");
                    assert_eq!(held, &[ADMITTED_NAME.to_owned()], "{at}");
                    assert_eq!(sequence, "spend/fake/1", "{at}");
                    assert_eq!(reservation, &format!("spend/reservations/{run}"), "{at}");
                    let stored = Reservation::from_bytes(stored).unwrap();
                    assert_eq!(reserved, std::slice::from_ref(&stored), "{at}");
                    assert_eq!(stored.run().as_str(), run, "{at}");
                    assert_eq!(stored.request(), Some(&id("order-17")), "{at}");
                    assert_eq!((stored.micros(), stored.start_ms()), (250_000, 7000));
                    let (before, asked_before) = (world.ran(), ledger.asked());
                    let again = admitting(&world, &ledger, store.clone());
                    let earlier = repeat(again.request(&asked("hello", "order-17")));
                    assert_eq!(world.ran(), before, "{at}");
                    assert_eq!(earlier.run().as_str(), run);
                    assert_eq!(earlier.state(), &RunState::NotEnded { here: false });
                    assert_eq!(ledger.asked(), asked_before, "{at}");
                    assert_eq!(ledger.reservations(store.as_ref()).unwrap(), reserved);
                    outcomes.insert("all of it");
                }
                other => panic!("{at}: a part of an admission: {other:?}"),
            }
        }
    }
    assert_eq!(
        outcomes.into_iter().collect::<Vec<_>>(),
        ["all of it", "nothing"]
    );
    // The masks were not a formality: some step left several directories and several
    // names undecided, and each of their choices was a crash of its own.
    assert!(
        most_dirs >= 2 && most_names >= 4,
        "{most_dirs} {most_names}"
    );
    assert!(crashes > 200, "{crashes}");
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
    // The same, with an ending that names the id and another run than the id's.
    let finished = ran(engine.request(&asked("hello", "order-22")));
    store.delete(&[finished.run().as_str()]).unwrap();
    let elsewhere = ENDING
        .replace("order-17", "order-22")
        .replace("7000-1", "9999-9");
    assert_ne!(finished.run().as_str(), "9999-9");
    plant("requests/order-22/ended", elsewhere.as_bytes());
    record_refused("order-22", "another request or another run");
    assert_eq!(world.ran(), 3);
    // An ending under an id that was never admitted: the id is not free, or its
    // action would run and its end then be refused.
    plant(
        "requests/order-30/ended",
        ENDING.replace("order-17", "order-30").as_bytes(),
    );
    record_refused("order-30", "an ending and no admission");
    assert_eq!(world.ran(), 3);
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

// --- the estimate, and a preparation handed to admission -----------------------------------------

/// Backends whose one model, and what it states as its bound, the test can change.
struct Priced {
    version: Mutex<Option<String>>,
    bound: AtomicU64,
}

impl Priced {
    fn new(bound: u64) -> Arc<Priced> {
        Arc::new(Priced {
            version: Mutex::new(Some("1".to_owned())),
            bound: AtomicU64::new(bound),
        })
    }
}

impl Backends for Priced {
    fn resolve(
        &self,
        _: Capability,
        _: Option<&str>,
        _: &[String],
    ) -> Result<ModelIdentity, Unavailable> {
        match self.version.lock().unwrap().as_deref() {
            Some(version) => Ok(ModelIdentity::new("fake", "fake-filler", version)),
            None => Err(Unavailable::new("the filler was taken away")),
        }
    }

    fn bound(&self, _: Capability, _: &ModelIdentity) -> Estimate {
        Estimate::of_micros(self.bound.load(Ordering::SeqCst))
    }
}

/// A config that the test can change.
struct Config(Mutex<Digest>);

impl ConfigSource for Config {
    fn digest(&self) -> Digest {
        *self.0.lock().unwrap()
    }
}

/// `retouch`: takes an image and a text, and needs a filler and a judge.
fn retouch(runs: &Arc<AtomicU64>) -> Arc<dyn Action> {
    let descriptor = ActionDescriptor::draft("retouch", "1.0.0", Kind::Primitive, "A test action.")
        .argument(Argument::required("text", "A text.", ValueType::Text))
        .input(InputPort::required(
            "image",
            whizbang_core::artifact::ArtifactType::Image,
            "The image.",
        ))
        .needs(Requirement::new(Capability::Fill))
        .needs(Requirement::new(Capability::Judge))
        .build()
        .unwrap();
    Arc::new(Paint {
        descriptor,
        runs: runs.clone(),
        body: Box::new(|_, _| Ok(Produced::nothing())),
    })
}

/// A world with two photographs in its store, and an executor with `retouch` over
/// backends and a config the test can change, and a ledger that counts.
struct Shop {
    world: World,
    store: Arc<Store>,
    backends: Arc<Priced>,
    config: Arc<Config>,
    ledger: Arc<FakeLedger>,
    engine: Engine,
    photo: ArtifactId,
    other: ArtifactId,
}

fn shop() -> Shop {
    use crate::artifact::support::{image_of, photo, png};
    let other_photo = || png(|x, y| [(y * 40) as u8, (x * 30) as u8, 7, 255]);
    use whizbang_core::artifact::{self, Artifact, ImageRole};
    let world = World::new();
    let store = world.store();
    let first = Artifact::from(image_of(photo(), ImageRole::Picture));
    let second = Artifact::from(image_of(other_photo(), ImageRole::Picture));
    let mut publication = store.begin().unwrap();
    let photo = artifact::put(&mut publication, &first).unwrap();
    let other = artifact::put(&mut publication, &second).unwrap();
    publication.publish().unwrap();
    let backends = Priced::new(250_000);
    let config = Arc::new(Config(Mutex::new(Digest::of_bytes(b"config"))));
    let ledger = Arc::new(FakeLedger::new());
    let mut registry = Registry::new();
    registry.register(retouch(&world.runs)).unwrap();
    let engine = Engine::over(
        registry,
        backends.clone(),
        store.clone(),
        Digest::of_bytes(b"unused"),
    )
    .with_config(config.clone())
    .with_clock(Arc::new(Stopped(7000)))
    .with_ledger(ledger.clone());
    Shop {
        world,
        store,
        backends,
        config,
        ledger,
        engine,
        photo,
        other,
    }
}

impl Shop {
    fn call(&self, image: ArtifactId) -> Call {
        Call::new("retouch")
            .argument("text", Value::text("hello"))
            .input("image", image)
    }

    /// The store's generation and its runs: what an admission would change.
    fn state(&self) -> (u64, usize, usize) {
        let generation = self.store.snapshot().unwrap();
        (
            generation.number(),
            generation.entries().len(),
            generation.runs().len(),
        )
    }

    /// What was reserved: the reservations the store names.
    fn reserved(&self) -> Vec<Reservation> {
        self.ledger.reservations(self.store.as_ref()).unwrap()
    }

    /// The request is refused as stale for this reason, and nothing came of it: no run,
    /// no reservation (the ledger was not asked for one, and the store names none
    /// more), no name in the store, no action run, and the id it came with still free.
    #[track_caller]
    fn refused_as_stale(&self, request: Request, why: Stale) {
        let before = self.state();
        let (asked, reserved) = (self.ledger.asked(), self.reserved());
        let request = request.with_id(id("order-17"));
        let refused = self.engine.request(&request).unwrap_err();
        assert_eq!(
            refused,
            CallError::Refused(Refusal::Stale {
                action: "retouch".to_owned(),
                why,
            })
        );
        assert_eq!(self.state(), before);
        assert_eq!(self.ledger.asked(), asked);
        assert_eq!(self.reserved(), reserved);
        assert_eq!(self.world.ran(), 0);
        assert!(
            self.store
                .snapshot()
                .unwrap()
                .get(&id("order-17").admission_name())
                .is_none()
        );
    }
}

#[test]
fn prepare_returns_an_estimate_with_readiness_and_it_is_not_in_the_fingerprint() {
    let shop = shop();
    let prepared = shop.engine.prepare(&shop.call(shop.photo)).unwrap();
    assert_eq!(prepared.readiness(), &Readiness::Ready);
    // Two needs, each answered by a model that states a bound of a quarter of a dollar.
    assert_eq!(prepared.estimate().micros(), 500_000);
    // The same models stating another bound: the same fingerprint, another estimate.
    shop.backends.bound.store(1, Ordering::SeqCst);
    let cheaper = shop.engine.prepare(&shop.call(shop.photo)).unwrap();
    assert_eq!(cheaper.estimate().micros(), 2);
    assert_eq!(cheaper.fingerprint(), prepared.fingerprint());
    // A need that nothing answers adds nothing, and the call is not ready.
    *shop.backends.version.lock().unwrap() = None;
    let unready = shop.engine.prepare(&shop.call(shop.photo)).unwrap();
    assert!(matches!(unready.readiness(), Readiness::NotReady(_)));
    assert_eq!(unready.estimate().micros(), 0);
    // Backends that state no bound cost nothing, and a sum that would overflow is the
    // largest amount there is, not a small one.
    let world = World::new();
    let free = world.engine(world.store()).prepare(&call("hello")).unwrap();
    assert_eq!(free.estimate().micros(), 0);
    *shop.backends.version.lock().unwrap() = Some("1".to_owned());
    shop.backends.bound.store(u64::MAX, Ordering::SeqCst);
    let most = shop.engine.prepare(&shop.call(shop.photo)).unwrap();
    assert_eq!(most.estimate().micros(), u64::MAX);
    // Preparing wrote nothing.
    assert_eq!(shop.state(), (1, 2, 0));
}

#[test]
fn a_preparation_that_still_holds_is_admitted_and_runs() {
    let shop = shop();
    let prepared = shop.engine.prepare(&shop.call(shop.photo)).unwrap();
    let request = Request::new(shop.call(shop.photo)).expecting(&prepared);
    assert_eq!(request.expected(), Some(&prepared));
    let finished = ran(shop
        .engine
        .request(&request.clone().with_id(id("order-17"))));
    assert_eq!(shop.world.ran(), 1);
    // The run was admitted with the fingerprint the caller held.
    let generation = shop.store.snapshot().unwrap();
    let admission = admitted(&generation, finished.run().as_str(), |e| shop.store.read(e));
    assert_eq!(
        admission.unwrap().unwrap().prepared(),
        prepared.fingerprint()
    );
    // The run ended: the store no longer counts it among those it has open.
    assert!(!shop.store.made_here(finished.run().as_str()));
    // Without an id, the same.
    ran(shop.engine.request(&request));
    assert_eq!(shop.world.ran(), 2);
}

#[test]
fn a_preparation_is_refused_at_admission_when_the_input_changed() {
    // An input is named by the digest of its content, so it cannot change under a
    // preparation. What can: the preparation the caller holds is of the call with
    // another input.
    let shop = shop();
    let prepared = shop.engine.prepare(&shop.call(shop.other)).unwrap();
    let request = Request::new(shop.call(shop.photo)).expecting(&prepared);
    shop.refused_as_stale(request, Stale::Inputs);
}

#[test]
fn a_preparation_is_refused_at_admission_when_its_input_is_no_longer_in_the_store() {
    // Or the input is gone: the store that the call is admitted against does not hold
    // it. Here the caller prepared against another store, which did.
    let shop = shop();
    let prepared = shop.engine.prepare(&shop.call(shop.photo)).unwrap();
    let elsewhere = World::new();
    let mut registry = Registry::new();
    registry.register(retouch(&elsewhere.runs)).unwrap();
    let empty = Engine::over(
        registry,
        shop.backends.clone(),
        elsewhere.store(),
        Digest::of_bytes(b"config"),
    )
    .with_ledger(shop.ledger.clone());
    let request = Request::new(shop.call(shop.photo))
        .expecting(&prepared)
        .with_id(id("order-17"));
    let refused = empty.request(&request).unwrap_err();
    let CallError::Refused(Refusal::Stale { action, why }) = &refused else {
        panic!("{refused:?}");
    };
    assert_eq!(action, "retouch");
    let Stale::Refused(now) = why else {
        panic!("{why:?}");
    };
    assert!(
        matches!(
            &**now,
            Refusal::Input {
                fault: InputFault::Missing,
                ..
            }
        ),
        "{now:?}"
    );
    assert!(
        refused.to_string().starts_with(
            "retouch was not admitted: the preparation it came with is stale: the call is \
             refused now: retouch: the input image: the store holds no image "
        ),
        "{refused}"
    );
    assert_eq!(
        held(&elsewhere.root(), elsewhere.fs.clone()),
        (0, vec![], vec![])
    );
    assert_eq!(elsewhere.ran(), 0);
    assert_eq!(shop.ledger.asked(), 0);
}

#[test]
fn a_preparation_is_refused_at_admission_when_the_config_changed() {
    // The config is read again at admission, not only when the executor was built.
    let shop = shop();
    let prepared = shop.engine.prepare(&shop.call(shop.photo)).unwrap();
    *shop.config.0.lock().unwrap() = Digest::of_bytes(b"another config");
    let request = Request::new(shop.call(shop.photo)).expecting(&prepared);
    shop.refused_as_stale(request.clone(), Stale::Config);
    let said = shop.engine.request(&request).unwrap_err().to_string();
    assert_eq!(
        said,
        "retouch was not admitted: the preparation it came with is stale: the config \
         changed since"
    );
    // Prepared again under the config as it is, the same call is admitted, under the
    // id the stale request did not take.
    let again = shop.engine.prepare(&shop.call(shop.photo)).unwrap();
    assert_ne!(again.fingerprint(), prepared.fingerprint());
    let request = Request::new(shop.call(shop.photo)).expecting(&again);
    ran(shop.engine.request(&request.with_id(id("order-17"))));
    assert_eq!(shop.world.ran(), 1);
    // And that is when its half dollar is reserved, once.
    assert_eq!(shop.ledger.asked(), 1);
    let reserved = shop.reserved();
    assert_eq!(reserved.len(), 1);
    assert_eq!(reserved[0].micros(), again.estimate().micros());
    assert_eq!(reserved[0].micros(), 500_000);
}

#[test]
fn a_preparation_is_refused_at_admission_when_the_model_identity_changed() {
    let shop = shop();
    let prepared = shop.engine.prepare(&shop.call(shop.photo)).unwrap();
    // The model that would answer has another version now.
    *shop.backends.version.lock().unwrap() = Some("2".to_owned());
    let request = Request::new(shop.call(shop.photo)).expecting(&prepared);
    let fill = Stale::Model {
        capability: Capability::Fill,
    };
    shop.refused_as_stale(request.clone(), fill.clone());
    // Or nothing answers any more: that is a stale preparation, said before the call
    // is said not to be ready.
    *shop.backends.version.lock().unwrap() = None;
    shop.refused_as_stale(request, fill);
    // A preparation that was itself not ready, and still is not, is refused as not
    // ready: nothing about it is stale.
    let unready = shop.engine.prepare(&shop.call(shop.photo)).unwrap();
    let request = Request::new(shop.call(shop.photo)).expecting(&unready);
    assert!(matches!(
        shop.engine.request(&request),
        Err(CallError::Refused(Refusal::NotReady { .. }))
    ));
    assert_eq!(shop.world.ran(), 0);
}

#[test]
fn a_preparation_is_refused_at_admission_when_it_is_of_another_call_or_its_bound_changed() {
    let shop = shop();
    let prepared = shop.engine.prepare(&shop.call(shop.photo)).unwrap();
    let other = shop
        .call(shop.photo)
        .argument("text", Value::text("goodbye"));
    shop.refused_as_stale(Request::new(other).expecting(&prepared), Stale::Call);
    // The same models under the same config state another bound: what would be
    // reserved is not what the caller was shown.
    shop.backends.bound.store(300_000, Ordering::SeqCst);
    let request = Request::new(shop.call(shop.photo)).expecting(&prepared);
    shop.refused_as_stale(request.clone(), Stale::Estimate);
    // A lower bound is another bound too.
    shop.backends.bound.store(200_000, Ordering::SeqCst);
    shop.refused_as_stale(request, Stale::Estimate);
}

#[test]
fn a_repeat_is_answered_before_its_preparation_is_looked_at() {
    // The id is looked up first: the same request again is answered with its run even
    // when the preparation it comes with has gone stale.
    let shop = shop();
    let prepared = shop.engine.prepare(&shop.call(shop.photo)).unwrap();
    let request = Request::new(shop.call(shop.photo))
        .expecting(&prepared)
        .with_id(id("order-17"));
    let finished = ran(shop.engine.request(&request));
    *shop.config.0.lock().unwrap() = Digest::of_bytes(b"another config");
    assert_eq!(repeat(shop.engine.request(&request)).run(), finished.run());
    assert_eq!(shop.world.ran(), 1);
}

#[test]
#[ignore = "a measurement, not a check: about a minute, and half a gigabyte"]
fn measure_what_preparing_and_admitting_one_large_input_costs() {
    // `docs/carry-over.md` asked for the cost of decoding an image input at prepare
    // and again at admission, with a figure. One input at the default limit of
    // 50,000,000 pixels; run with `--ignored --nocapture`, in the profile to be measured.
    use std::io::Cursor;
    use std::time::Instant;
    use whizbang_core::artifact::{self, Artifact, Image, ImageRole, Origin, Step};
    use whizbang_core::identity::{DecodeLimits, Projection, Topology};
    let picture = image::RgbaImage::from_fn(8000, 6250, |x, y| {
        image::Rgba([(x % 251) as u8, (y % 241) as u8, ((x ^ y) % 239) as u8, 255])
    });
    let mut file = Vec::new();
    picture
        .write_to(&mut Cursor::new(&mut file), image::ImageFormat::Png)
        .unwrap();
    drop(picture);
    let image = Image::new(
        file,
        Projection::Flat,
        Topology::Bounded,
        ImageRole::Picture,
        Origin::new(Step::new("import", "1")).unwrap(),
        &DecodeLimits::default(),
    )
    .unwrap();
    let world = World::new();
    let store = Arc::new(Store::open(&world.root(), Arc::new(RealFs)).unwrap());
    let mut publication = store.begin().unwrap();
    let id = artifact::put(&mut publication, &Artifact::from(image)).unwrap();
    publication.publish().unwrap();
    let mut registry = Registry::new();
    registry.register(retouch(&world.runs)).unwrap();
    let engine = Engine::over(registry, Arc::new(OneModel), store, Digest::of_bytes(b""));
    let call = Call::new("retouch")
        .argument("text", Value::text("hello"))
        .input("image", id);
    let start = Instant::now();
    let prepared = engine.prepare(&call).unwrap();
    let preparing = start.elapsed();
    let start = Instant::now();
    ran(engine.request(&Request::new(call).expecting(&prepared)));
    let admitting = start.elapsed();
    println!("prepare: {preparing:?}; request, which prepares again: {admitting:?}");
}

// --- follow-ups of the first review --------------------------------------------------------------

#[test]
fn a_repeat_is_not_answered_from_another_run_that_took_the_deleted_runs_id() {
    // A run's id is free again once the run is deleted, and a later process can give it
    // to another run. The id's record names the run by its id, so the run that holds
    // that id now is taken for the request's only when it holds the very admission the
    // id's record is: otherwise the request's run is gone, and the ending answers.
    let world = World::new();
    let store = world.store();
    let engine = world.engine(store.clone());
    let failing = Request::new(call("hello").argument("fail", Value::text("no paint left")))
        .with_id(id("order-17"));
    let failed = ran(engine.request(&failing));
    let other = engine.submit(&call("something else")).unwrap();
    assert_eq!(other.outcome(), &completed());
    // The other run's admission and record, as a later process would have written them
    // had it named its run as the first was named.
    let (old, new) = (other.run().as_str(), failed.run().as_str());
    assert_eq!(old.len(), new.len());
    let generation = store.snapshot().unwrap();
    let theirs = generation.run(old).unwrap();
    let renamed: Vec<(String, Vec<u8>)> = theirs
        .entries()
        .iter()
        .map(|(name, entry)| {
            let text = String::from_utf8(store.read(entry).unwrap()).unwrap();
            let named = format!(r#""run":"{old}""#);
            (
                name.clone(),
                text.replace(&named, &format!(r#""run":"{new}""#))
                    .into_bytes(),
            )
        })
        .collect();
    store.delete(&[new]).unwrap();
    let mut publication = store.begin().unwrap();
    publication.create_run(new).unwrap();
    for (name, bytes) in &renamed {
        publication
            .put_in(new, name, store::Kind::Diagnostic, bytes)
            .unwrap();
    }
    publication.publish().unwrap();
    // The store holds a completed run under the id of the failed one.
    let generation = store.snapshot().unwrap();
    let record = record::stored(&generation, new, |e| store.read(e))
        .unwrap()
        .unwrap();
    assert_eq!(record.outcome().status, Status::Completed);

    let earlier = repeat(engine.request(&failing));
    assert_eq!(earlier.run(), failed.run());
    assert_eq!(
        earlier.state(),
        &RunState::Removed {
            ended: Some(Status::Error)
        }
    );
    // The same when the run that took the id holds no admission at all.
    store.delete(&[new]).unwrap();
    let mut publication = store.begin().unwrap();
    publication
        .put_in(new, "notes", store::Kind::Diagnostic, b"x")
        .unwrap();
    publication.publish().unwrap();
    assert_eq!(
        repeat(engine.request(&failing)).state(),
        &RunState::Removed {
            ended: Some(Status::Error)
        }
    );
    assert_eq!(world.ran(), 2);
}

#[test]
fn an_admission_that_was_visible_and_not_confirmed_takes_the_id_and_never_runs() {
    // The last sync of the admission's publication is refused: the run is visible, the
    // caller is told of the store's failure, and the action does not start. The id is
    // taken all the same: the same request again is answered with that run, not ended,
    // and another request under the id is a conflict.
    let world = World::new();
    let store = world.store();
    let engine = world.engine(store);
    let root = world.root();
    world.fs.refuse_matching(
        0,
        move |event| matches!(event, Event::SyncDir(dir) if *dir == root),
    );
    let refused = engine.request(&asked("hello", "order-17")).unwrap_err();
    assert!(
        refused.to_string().contains("not confirmed durable"),
        "{refused}"
    );
    assert_eq!(world.ran(), 0);
    let earlier = repeat(engine.request(&asked("hello", "order-17")));
    // Abandoned (ADR 0008, "A run with nothing behind it"): open here, and nothing ends it.
    assert!(
        matches!(earlier.state(), RunState::Abandoned { failure: Some(_) }),
        "{:?}",
        earlier.state()
    );
    assert!(matches!(
        engine.request(&asked("goodbye", "order-17")),
        Err(CallError::Refused(Refusal::Conflict { .. }))
    ));
    assert_eq!(world.ran(), 0);
}

#[test]
fn a_request_id_is_made_from_text_through_the_executor_which_shows_no_secret() {
    // A door hands the executor the text a caller gave. Text that holds a secret is
    // refused before anything of it is shown, whether or not it would be an id; other
    // text that is no id is refused with what it was, escaped and cut.
    let key = made_up_key("");
    let world = World::new();
    let engine = world
        .engine(world.store())
        .with_secrets(Secrets::new([key.clone()]).unwrap());
    for held in [key.clone(), format!("not an id {key}"), format!("{key}/..")] {
        let refused = engine.request_id(&held).unwrap_err();
        assert_eq!(refused, CallError::Refused(Refusal::SecretInRequestId));
        assert!(!format!("{refused} {refused:?}").contains(&key));
    }
    let refused = engine.request_id("not an id").unwrap_err();
    let CallError::Refused(Refusal::NotARequestId { why }) = &refused else {
        panic!("{refused:?}");
    };
    assert!(why.contains("\"not an id\" is no request id"), "{why}");
    assert_eq!(&refused.to_string(), why);
    assert_eq!(engine.request_id("order-17").unwrap(), id("order-17"));
}

/// The real file system, which runs a hook once, before the first read of stored
/// content after it was armed: between a look-up's snapshot and its first record.
struct Between {
    inner: RealFs,
    hook: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

impl Fs for Between {
    fn create_dir(&self, path: &Path) -> std::io::Result<()> {
        self.inner.create_dir(path)
    }
    fn write_new(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
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
        if path.parent().is_some_and(|dir| dir.ends_with("objects")) {
            let hook = self.hook.lock().unwrap().take();
            if let Some(hook) = hook {
                hook();
            }
        }
        self.inner.read(path)
    }
    fn list(&self, path: &Path) -> std::io::Result<Vec<PathBuf>> {
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
fn a_repeat_that_races_the_end_of_its_run_is_not_told_the_run_will_never_end() {
    // The repeat takes its snapshot while the run executes. Before it reads the id's
    // record, the run ends: its end is published and the store forgets the run. The
    // snapshot shows a run with no record, and the store no longer says it made it;
    // the answer must not be that the run's process is gone.
    let dir = Scratch::new("admit-race");
    let fs = Arc::new(Between {
        inner: RealFs,
        hook: Mutex::new(None),
    });
    let store = Arc::new(Store::open(&dir.path().join("store"), fs.clone()).unwrap());
    let runs = Arc::new(AtomicU64::new(0));
    let (started, has_started) = mpsc::channel::<()>();
    let (release, released) = mpsc::channel::<()>();
    let (started, released) = (Mutex::new(started), Mutex::new(released));
    let held = paint_with(&runs, move |_, _| {
        started.lock().unwrap().send(()).unwrap();
        released.lock().unwrap().recv().unwrap();
        Ok(Produced::nothing())
    });
    let first = engine(store.clone(), vec![held]);
    let second = engine(store.clone(), vec![paint(&runs)]);
    let (ended, has_ended) = mpsc::channel::<()>();
    whizbang_testkit::threads::scope(|scope| {
        let running = scope.spawn(|| {
            let finished = ran(first.request(&asked("hello", "order-17")));
            ended.send(()).unwrap();
            finished
        });
        has_started.recv().unwrap();
        *fs.hook.lock().unwrap() = Some(Box::new(move || {
            release.send(()).unwrap();
            has_ended.recv().unwrap();
        }));
        let earlier = repeat(second.request(&asked("hello", "order-17")));
        let finished = running.join().unwrap();
        assert!(fs.hook.lock().unwrap().is_none(), "the hook never ran");
        assert_eq!(earlier.run(), finished.run());
        assert_eq!(earlier.state(), &RunState::Ended(completed()));
    });
    assert_eq!(runs.load(Ordering::SeqCst), 1);
}

// --- the reservation -----------------------------------------------------------------------------

const RESERVATION: &str = concat!(
    r#"{"record":"reservation/1","run":"7000-1","request":"order-17","#,
    r#""admission":"sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","#,
    r#""reserved_micros":500000,"start_ms":7000}"#,
    "\n"
);

#[test]
fn a_reservation_is_written_one_way_and_read_back_strictly() {
    let reservation = Reservation::from_bytes(RESERVATION.as_bytes()).unwrap();
    assert_eq!(reservation.run().as_str(), "7000-1");
    assert_eq!(reservation.request(), Some(&id("order-17")));
    assert_eq!(reservation.admission(), Digest::of_bytes(b""));
    assert_eq!(
        (reservation.micros(), reservation.start_ms()),
        (500_000, 7000)
    );
    assert_eq!(reservation.name(), "spend/reservations/7000-1");
    assert_eq!(reservation.to_bytes(), RESERVATION.as_bytes());
    let without = RESERVATION.replace(r#""request":"order-17""#, r#""request":null"#);
    let reservation = Reservation::from_bytes(without.as_bytes()).unwrap();
    assert_eq!(reservation.request(), None);
    assert_eq!(reservation.to_bytes(), without.as_bytes());
    // One under the largest amount there is, which is itself no amount.
    let most = RESERVATION.replace("500000", &(u64::MAX - 1).to_string());
    let reservation = Reservation::from_bytes(most.as_bytes()).unwrap();
    assert_eq!(reservation.micros(), u64::MAX - 1);

    type Fault = fn(&RequestRecordError) -> bool;
    let malformed: Fault = |e| matches!(e, RequestRecordError::Malformed(_));
    let inconsistent: Fault = |e| matches!(e, RequestRecordError::Inconsistent(_));
    let other_version: Fault = |e| matches!(e, RequestRecordError::OtherVersion { .. });
    let not_canonical: Fault = |e| matches!(e, RequestRecordError::NotCanonical);
    let too_large: Fault = |e| matches!(e, RequestRecordError::TooLarge { .. });
    let control = r"\u001b[31m".repeat(30);
    let cases: Vec<(&str, String, Fault)> = vec![
        ("no bytes", String::new(), malformed),
        ("not JSON", "not json\n".to_owned(), malformed),
        ("an admission", ADMISSION.to_owned(), other_version),
        (
            "another version",
            RESERVATION.replace("reservation/1", "reservation/2"),
            other_version,
        ),
        (
            "another kind",
            RESERVATION.replace("reservation/1", &control),
            other_version,
        ),
        (
            "an unknown field",
            RESERVATION.replace(r#""reserved_micros""#, r#""settled":1,"reserved_micros""#),
            malformed,
        ),
        (
            "a field left out",
            RESERVATION.replace(r#""request":"order-17","#, ""),
            malformed,
        ),
        (
            "a field twice",
            RESERVATION.replace(
                r#""reserved_micros":500000,"#,
                r#""reserved_micros":1,"reserved_micros":500000,"#,
            ),
            malformed,
        ),
        (
            "a run that is no run id",
            RESERVATION.replace("7000-1", "../7000"),
            inconsistent,
        ),
        (
            "a request that is no request id",
            RESERVATION.replace("order-17", "order 17"),
            inconsistent,
        ),
        (
            "a digest that is none",
            RESERVATION.replace("sha256:e3b0", "sha256:zzzz"),
            malformed,
        ),
        (
            "an amount under zero",
            RESERVATION.replace("500000", "-1"),
            malformed,
        ),
        (
            "the largest amount there is, which is no amount",
            RESERVATION.replace("500000", "18446744073709551615"),
            inconsistent,
        ),
        (
            "an amount past the largest",
            RESERVATION.replace("500000", "18446744073709551616"),
            malformed,
        ),
        (
            "an amount that is not whole",
            RESERVATION.replace("500000", "0.5"),
            malformed,
        ),
        (
            "an amount with a zero before it",
            RESERVATION.replace("500000", "0500000"),
            malformed,
        ),
        (
            "an amount written another way",
            RESERVATION.replace("500000", "5e5"),
            malformed,
        ),
        (
            "a space between tokens",
            RESERVATION.replace(r#","run""#, r#", "run""#),
            not_canonical,
        ),
        (
            "no newline",
            RESERVATION.trim_end().to_owned(),
            not_canonical,
        ),
        (
            "something after the record",
            format!("{RESERVATION}{RESERVATION}"),
            malformed,
        ),
        (
            "more bytes than a record may be",
            RESERVATION.replace("order-17", &"x".repeat(2000)),
            too_large,
        ),
    ];
    for (what, bytes, fault) in &cases {
        let error = Reservation::from_bytes(bytes.as_bytes()).unwrap_err();
        assert!(fault(&error), "{what}: {error:?}");
        let said = error.to_string();
        assert!(!said.chars().any(char::is_control), "{what}: {said:?}");
        assert!(said.len() < 500, "{what}: {}", said.len());
    }
    // Neither of the other two records is read as a reservation, nor it as them.
    assert!(other_version(
        &Admission::from_bytes(RESERVATION.as_bytes()).unwrap_err()
    ));
    assert!(other_version(
        &Reservation::from_bytes(ENDING.as_bytes()).unwrap_err()
    ));
}

/// The content of every file under the store's `objects`, whatever names it or does
/// not, that is a reservation.
fn reservations_in_objects(root: &Path) -> Vec<Reservation> {
    files(&root.join("objects"))
        .into_iter()
        .filter_map(|(_, bytes)| Reservation::from_bytes(&bytes).ok())
        .collect()
}

#[test]
fn admission_reserves_the_estimate_in_the_step_that_makes_the_run_and_the_ids_record() {
    let shop = shop();
    assert_eq!(shop.state(), (1, 2, 0));
    let prepared = shop.engine.prepare(&shop.call(shop.photo)).unwrap();
    assert_eq!(prepared.estimate().micros(), 500_000);
    assert_eq!((shop.ledger.asked(), shop.reserved()), (0, vec![]));

    let request = Request::new(shop.call(shop.photo))
        .expecting(&prepared)
        .with_id(id("order-17"));
    let finished = ran(shop.engine.request(&request));
    let run = finished.run().clone();
    assert_eq!(shop.ledger.asked(), 1);

    // The admission was one generation: the run, the id's record, the reservation and
    // the ledger's own record came with it together. The end was the next.
    let reader = Reader::open(&shop.world.root(), shop.world.fs.clone()).unwrap();
    let admitted_at = Reader::open(&shop.world.root(), shop.world.fs.clone()).unwrap();
    let generation = admitted_at.snapshot().unwrap();
    assert_eq!(generation.number(), 3);
    let names: Vec<&str> = generation
        .entries()
        .keys()
        .map(String::as_str)
        .filter(|name| !name.starts_with("artifacts/"))
        .collect();
    let reservation_name = format!("spend/reservations/{run}");
    assert_eq!(
        names,
        [
            "requests/order-17/admitted",
            "requests/order-17/ended",
            "spend/fake/1",
            reservation_name.as_str(),
        ]
    );
    // Generation 2, the admission's, is still on the disk: it holds the run with its
    // admission alone, and every name but the ending.
    let second = std::fs::read(shop.world.root().join("generations").join("2.json")).unwrap();
    let second: serde_json::Value = serde_json::from_slice(&second).unwrap();
    for name in [names[0], names[2], names[3]] {
        assert!(second["entries"].get(name).is_some(), "{name}: {second}");
    }
    assert!(second["entries"].get(names[1]).is_none());
    let first = std::fs::read(shop.world.root().join("generations").join("1.json")).unwrap();
    let first: serde_json::Value = serde_json::from_slice(&first).unwrap();
    for name in &names {
        assert!(first["entries"].get(*name).is_none(), "{name}");
    }

    // The reservation: of this run and this request, for the estimate, in a name made
    // from the run's id, and holding the digest of the run's own admission.
    let reserved = shop.reserved();
    assert_eq!(reserved.len(), 1);
    let reservation = &reserved[0];
    assert_eq!(reservation.run(), &run);
    assert_eq!(reservation.request(), Some(&id("order-17")));
    assert_eq!(reservation.micros(), 500_000);
    assert_eq!(reservation.start_ms(), 7000);
    assert_eq!(Reservation::name_of(&run), reservation_name);
    let entry = generation.get(&reservation_name).unwrap();
    assert_eq!(entry.kind, store::Kind::Diagnostic);
    let own = generation.run(run.as_str()).unwrap();
    let admitted_entry = own.get(ADMITTED_NAME).unwrap();
    assert_eq!(reservation.admission(), admitted_entry.digest);
    // The admission says what was reserved, in both its copies, which are one content.
    let outside = generation.get("requests/order-17/admitted").unwrap();
    assert_eq!(outside.digest, admitted_entry.digest);
    let admission = Admission::from_bytes(&reader.read(outside).unwrap()).unwrap();
    assert_eq!(admission.reserved_micros(), Some(500_000));
    assert_eq!(admission.run(), &run);
    // And the run's record says it too.
    let record = record::stored(&generation, run.as_str(), |e| reader.read(e))
        .unwrap()
        .unwrap();
    assert_eq!(record.spend().reserved_micros, Some(500_000));

    // The same request again reserves nothing more, and the ledger is not asked.
    assert_eq!(repeat(shop.engine.request(&request)).run(), &run);
    assert_eq!((shop.ledger.asked(), shop.reserved().len()), (1, 1));
}

#[test]
fn a_call_the_ledger_declines_is_refused_with_no_run_and_its_id_stays_free() {
    let shop = shop();
    // The call may cost half a dollar, and forty cents may be reserved.
    shop.ledger.allow(Some(400_000));
    let request = Request::new(shop.call(shop.photo)).with_id(id("order-17"));
    let before = (shop.state(), files(&shop.world.root()).len());
    let refused = shop.engine.request(&request).unwrap_err();
    assert_eq!(
        refused,
        CallError::Refused(Refusal::OverAllowance {
            action: "retouch".to_owned(),
            asked_micros: 500_000,
            left_micros: 400_000,
        })
    );
    assert_eq!(
        refused.to_string(),
        "retouch was not admitted: it may cost up to 500000 millionths of a US dollar, and \
         400000 are left of the ledger's allowance"
    );
    // The ledger was asked, and nothing came of it: no generation, no run, no name, no
    // file, no action run.
    assert_eq!(shop.ledger.asked(), 1);
    assert_eq!((shop.state(), files(&shop.world.root()).len()), before);
    assert_eq!(shop.reserved(), []);
    assert_eq!(reservations_in_objects(&shop.world.root()), []);
    assert_eq!(shop.world.ran(), 0);
    // The same without an id.
    assert!(matches!(
        shop.engine.submit(&shop.call(shop.photo)),
        Err(CallError::Refused(Refusal::OverAllowance { .. }))
    ));
    assert_eq!((shop.state(), files(&shop.world.root()).len()), before);

    // The id is free: with a dollar and twenty cents allowed, the same request is
    // admitted under it, and a second call too. A third is refused for what is left.
    shop.ledger.allow(Some(1_200_000));
    let first = ran(shop.engine.request(&request));
    let second = shop.engine.submit(&shop.call(shop.other)).unwrap();
    let refused = shop.engine.submit(&shop.call(shop.photo)).unwrap_err();
    assert_eq!(
        refused.refusal(),
        Some(&Refusal::OverAllowance {
            action: "retouch".to_owned(),
            asked_micros: 500_000,
            left_micros: 200_000,
        })
    );
    let reserved: Vec<_> = shop.reserved().iter().map(|r| r.run().clone()).collect();
    let mut made = vec![first.run().clone(), second.run().clone()];
    made.sort();
    assert_eq!(reserved, made);
    assert_eq!(shop.world.ran(), 2);
    // An exact fit is taken.
    shop.backends.bound.store(100_000, Ordering::SeqCst);
    shop.engine.submit(&shop.call(shop.photo)).unwrap();
    assert_eq!(shop.reserved().len(), 3);
}

#[test]
fn two_simultaneous_requests_with_one_id_make_one_reservation() {
    // Two executors over one store and one ledger, held together once each has looked
    // the id up and prepared. Each asks the ledger, and each stages a reservation for
    // the run it named. The store admits one. The other's reservation is content that
    // no name leads to: it is not counted, and a prune removes it.
    let world = World::new();
    let store = world.store();
    let clock = Arc::new(Together::new(2));
    let ledger = Arc::new(FakeLedger::new());
    // Each waits inside the ledger too, so that both reservations are staged before
    // either admission is published.
    let both = Arc::new(Waiting::new(ledger.clone(), 2));
    let engines: Vec<Engine> = (0..2)
        .map(|_| {
            engine_with(store.clone(), &world.runs, Priced::new(250_000))
                .with_clock(clock.clone())
                .with_ledger(both.clone())
        })
        .collect();
    let answers: Vec<Answer> = whizbang_testkit::threads::scope(|scope| {
        let threads: Vec<_> = engines
            .iter()
            .map(|engine| scope.spawn(move || engine.request(&asked("hello", "order-17")).unwrap()))
            .collect();
        threads.into_iter().map(|t| t.join().unwrap()).collect()
    });
    let made: Vec<_> = answers
        .iter()
        .filter(|a| matches!(a, Answer::Ran(_)))
        .collect();
    assert_eq!(made.len(), 1, "{answers:?}");
    assert_eq!(answers[0].run(), answers[1].run());
    assert_eq!(world.ran(), 1);
    // Asked twice; one reservation, of the run that was made.
    assert_eq!(ledger.asked(), 2);
    let reserved = ledger.reservations(store.as_ref()).unwrap();
    assert_eq!(reserved.len(), 1, "{reserved:?}");
    assert_eq!(reserved[0].run(), made[0].run());
    assert_eq!(reserved[0].request(), Some(&id("order-17")));
    // The refused attempt's reservation is in the store's content all the same, for
    // another run: whatever counted content, and not names, would count two.
    let in_objects = reservations_in_objects(&world.root());
    assert_eq!(in_objects.len(), 2, "{in_objects:?}");
    assert!(in_objects.contains(&reserved[0]));
    let orphan = in_objects.iter().find(|r| **r != reserved[0]).unwrap();
    assert_ne!(orphan.run(), reserved[0].run());
    assert!(
        store
            .snapshot()
            .unwrap()
            .run(orphan.run().as_str())
            .is_none()
    );
    store.prune().unwrap();
    assert_eq!(reservations_in_objects(&world.root()), reserved);
    assert_eq!(ledger.reservations(store.as_ref()).unwrap(), reserved);
}

/// A ledger that holds the first callers, once the ledger behind it has answered each,
/// until as many are there as the barrier takes: each has judged, and staged what it
/// adds, on a generation in which none of the others has reserved.
struct Waiting {
    inner: Arc<FakeLedger>,
    barrier: Barrier,
    left: Mutex<usize>,
}

impl Waiting {
    fn new(inner: Arc<FakeLedger>, callers: usize) -> Waiting {
        Waiting {
            inner,
            barrier: Barrier::new(callers),
            left: Mutex::new(callers),
        }
    }
}

impl Ledger for Waiting {
    fn reserve(&self, reservation: &Reservation, step: &mut Step<'_, '_>) -> Result<(), Declined> {
        let answer = self.inner.reserve(reservation, step);
        let waits = {
            let mut left = self.left.lock().unwrap();
            let waits = *left > 0;
            *left = left.saturating_sub(1);
            waits
        };
        if waits {
            self.barrier.wait();
        }
        answer
    }
}

#[test]
fn two_simultaneous_admissions_cannot_both_take_the_last_of_the_allowance() {
    // Two requests under two ids, each for a quarter of a dollar, and forty cents may
    // be reserved. Each asks the ledger on a generation in which nothing is reserved,
    // and each is told yes. Both cannot be published: the ledger made the same name in
    // both, the store refuses the later whole, and the executor asks the ledger again,
    // on the generation as it is then. That one is declined.
    let world = World::new();
    let store = world.store();
    let ledger = Arc::new(FakeLedger::with_allowance(400_000));
    let both = Arc::new(Waiting::new(ledger.clone(), 2));
    let engines: Vec<Engine> = (0..2)
        .map(|_| {
            engine_with(store.clone(), &world.runs, Priced::new(250_000)).with_ledger(both.clone())
        })
        .collect();
    let answers: Vec<Result<Answer, CallError>> = whizbang_testkit::threads::scope(|scope| {
        let threads: Vec<_> = engines
            .iter()
            .zip(["order-17", "order-18"])
            .map(|(engine, id)| scope.spawn(move || engine.request(&asked("hello", id))))
            .collect();
        threads.into_iter().map(|t| t.join().unwrap()).collect()
    });
    let made: Vec<_> = answers.iter().filter_map(|a| a.as_ref().ok()).collect();
    let refused: Vec<_> = answers.iter().filter_map(|a| a.as_ref().err()).collect();
    assert_eq!((made.len(), refused.len()), (1, 1), "{answers:?}");
    assert_eq!(
        refused[0].refusal(),
        Some(&Refusal::OverAllowance {
            action: "paint".to_owned(),
            asked_micros: 250_000,
            left_micros: 150_000,
        })
    );
    assert_eq!(ledger.asked(), 3);
    let reserved = ledger.reservations(store.as_ref()).unwrap();
    assert_eq!(reserved.len(), 1);
    assert_eq!(reserved[0].run(), made[0].run());
    assert_eq!(world.ran(), 1);
    // One run, one id's two records, the ledger's record and the reservation: the
    // refused request left no name, and its id is free.
    let (_, names, runs) = held(&world.root(), world.fs.clone());
    assert_eq!((names.len(), runs.len()), (4, 1), "{names:?}");
    let taken = made[0].run();
    let free = answers.iter().position(|a| a.is_err()).unwrap();
    let free = ["order-17", "order-18"][free];
    assert!(names.iter().all(|(name, _)| !name.contains(free)));
    assert!(runs.iter().all(|(id, _)| id == taken.as_str()));
}

#[test]
fn a_run_that_is_named_again_has_one_reservation_which_names_its_final_run() {
    // The id the executor chose for the run is taken three times, each time as its
    // admission is about to be written. The admission and the reservation are made
    // again for each new id, and the ledger is asked each time. What the store names
    // is one reservation, of the run that was made, with that run's own admission.
    let dir = Scratch::new("admit-renamed");
    let root = dir.path().join("store");
    let (fs, store) = Taker::store(&root, 3);
    let runs = Arc::new(AtomicU64::new(0));
    let ledger = Arc::new(FakeLedger::new());
    let engine =
        engine_with(store.clone(), &runs, Priced::new(250_000)).with_ledger(ledger.clone());
    let finished = ran(engine.request(&asked("hello", "order-17")));
    let taken = fs.taken();
    assert_eq!(taken.len(), 3);
    assert!(!taken.iter().any(|id| id == finished.run().as_str()));
    assert_eq!(ledger.asked(), 4);

    let reserved = ledger.reservations(store.as_ref()).unwrap();
    assert_eq!(reserved.len(), 1, "{reserved:?}");
    assert_eq!(reserved[0].run(), finished.run());
    let generation = store.snapshot().unwrap();
    let own = generation.run(finished.run().as_str()).unwrap();
    let admitted_entry = own.get(ADMITTED_NAME).unwrap();
    assert_eq!(reserved[0].admission(), admitted_entry.digest);
    let outside = generation.get("requests/order-17/admitted").unwrap();
    assert_eq!(outside.digest, admitted_entry.digest);
    let admission = Admission::from_bytes(&store.read(outside).unwrap()).unwrap();
    assert_eq!(admission.run(), finished.run());
    assert_eq!(admission.reserved_micros(), Some(250_000));
    // The id's record and the run's own admission are the same bytes, and both name
    // the run that was made.
    let inside = store.read(admitted_entry).unwrap();
    assert_eq!(inside, store.read(outside).unwrap());
    assert_eq!(
        Admission::from_bytes(&inside).unwrap().run(),
        finished.run()
    );
    let spend: Vec<&String> = generation
        .entries()
        .keys()
        .filter(|name| name.starts_with("spend/"))
        .collect();
    assert_eq!(
        spend,
        [
            "spend/fake/1",
            &format!("spend/reservations/{}", finished.run())
        ]
    );
    // The three refused attempts each left a reservation in the store's content, of an
    // id that other hands hold: four by content, one by name.
    let in_objects = reservations_in_objects(&root);
    assert_eq!(in_objects.len(), 4, "{in_objects:?}");
    for id in &taken {
        assert!(in_objects.iter().any(|r| r.run().as_str() == id), "{id}");
    }
    // The same request again is answered with the run, from the two copies of the
    // admission, which were made again together.
    assert_eq!(
        repeat(engine.request(&asked("hello", "order-17"))).run(),
        finished.run()
    );
    assert_eq!(ledger.asked(), 4);
}

#[test]
fn a_run_that_is_named_eight_times_and_given_up_reserves_nothing() {
    let dir = Scratch::new("admit-given-up");
    let root = dir.path().join("store");
    let (fs, store) = Taker::store(&root, 8);
    let runs = Arc::new(AtomicU64::new(0));
    let ledger = Arc::new(FakeLedger::new());
    let engine =
        engine_with(store.clone(), &runs, Priced::new(250_000)).with_ledger(ledger.clone());
    let refused = engine.request(&asked("hello", "order-17")).unwrap_err();
    let CallError::Store(failure) = &refused else {
        panic!("{refused:?}");
    };
    assert!(matches!(failure.cause(), store::Error::RunExists { .. }));
    assert_eq!(fs.taken().len(), 8);
    assert_eq!(ledger.asked(), 8);
    assert_eq!(ledger.reservations(store.as_ref()).unwrap(), []);
    assert_eq!(runs.load(Ordering::SeqCst), 0);
    // Nothing outside the eight runs of other hands: no id's record, no name of spend.
    let generation = store.snapshot().unwrap();
    assert_eq!(generation.entries().len(), 0);
    assert_eq!(generation.runs().len(), 8);
    assert_eq!(reservations_in_objects(&root).len(), 8);
    // The id is free, and the request is admitted under it once ids are.
    let finished = ran(engine.request(&asked("hello", "order-17")));
    let reserved = ledger.reservations(store.as_ref()).unwrap();
    assert_eq!(reserved.len(), 1);
    assert_eq!(reserved[0].run(), finished.run());
}

#[test]
fn a_reservation_left_by_a_deleted_run_keeps_its_name_and_the_new_run_is_named_again() {
    // A run's id is free once the run is deleted, and its reservation is not removed.
    // A run that would be given that id cannot make the reservation's name: the store
    // refuses the admission whole, and the run is named again.
    let dir = Scratch::new("admit-reservation-taken");
    let root = dir.path().join("store");
    let (fs, store) = Taker::store_taking_reservations(&root, 2);
    let runs = Arc::new(AtomicU64::new(0));
    let ledger = Arc::new(FakeLedger::new());
    let engine =
        engine_with(store.clone(), &runs, Priced::new(250_000)).with_ledger(ledger.clone());
    let finished = ran(engine.request(&asked("hello", "order-17")));
    let taken = fs.taken();
    assert_eq!(taken.len(), 2);
    assert!(!taken.iter().any(|id| id == finished.run().as_str()));
    assert_eq!(ledger.asked(), 3);
    // The two old reservations are as they were, and the run's own is the third.
    let reserved = ledger.reservations(store.as_ref()).unwrap();
    assert_eq!(reserved.len(), 3);
    let own: Vec<_> = reserved
        .iter()
        .filter(|r| r.run() == finished.run())
        .collect();
    assert_eq!(own.len(), 1);
    assert_eq!(own[0].micros(), 250_000);
    for old in reserved.iter().filter(|r| r.run() != finished.run()) {
        assert_eq!((old.micros(), old.start_ms()), (1, 1));
    }
    assert_eq!(store.snapshot().unwrap().runs().len(), 1);

    // These namings are among the eight a run is given: with the name taken eight
    // times the store's refusal is what the caller is told, and nothing was admitted.
    let dir = Scratch::new("admit-reservation-taken-8");
    let root = dir.path().join("store");
    let (fs, store) = Taker::store_taking_reservations(&root, 8);
    let engine = engine_with(store.clone(), &runs, Priced::new(250_000))
        .with_ledger(Arc::new(FakeLedger::new()));
    let refused = engine.request(&asked("hello", "order-17")).unwrap_err();
    let CallError::Store(failure) = &refused else {
        panic!("{refused:?}");
    };
    assert!(matches!(failure.cause(), store::Error::NameExists { .. }));
    assert_eq!(fs.taken().len(), 8);
    let generation = store.snapshot().unwrap();
    assert_eq!(
        (generation.entries().len(), generation.runs().len()),
        (8, 0)
    );
    assert_eq!(runs.load(Ordering::SeqCst), 1);
}

#[test]
fn a_repeat_a_conflict_and_a_request_that_does_not_run_reserve_nothing() {
    let shop = shop();
    let request = Request::new(shop.call(shop.photo)).with_id(id("order-17"));
    let finished = ran(shop.engine.request(&request));
    let reserved = shop.reserved();
    assert_eq!((shop.ledger.asked(), reserved.len()), (1, 1));
    #[track_caller]
    fn unchanged(shop: &Shop, reserved: &[Reservation]) {
        assert_eq!(shop.ledger.asked(), 1);
        assert_eq!(shop.reserved(), reserved);
        assert_eq!(reservations_in_objects(&shop.world.root()), reserved);
    }

    // A repeat.
    assert_eq!(repeat(shop.engine.request(&request)).run(), finished.run());
    unchanged(&shop, &reserved);
    // A conflict.
    let other = Request::new(shop.call(shop.other)).with_id(id("order-17"));
    assert!(matches!(
        shop.engine.request(&other),
        Err(CallError::Refused(Refusal::Conflict { .. }))
    ));
    unchanged(&shop, &reserved);
    // A call that is no call of the action, and one of no action.
    let invalid = Request::new(Call::new("retouch")).with_id(id("order-18"));
    assert!(matches!(
        shop.engine.request(&invalid),
        Err(CallError::Refused(Refusal::Invalid { .. }))
    ));
    assert!(shop.engine.submit(&Call::new("vanish")).is_err());
    unchanged(&shop, &reserved);
    // A request that is not ready.
    *shop.backends.version.lock().unwrap() = None;
    let unready = Request::new(shop.call(shop.photo)).with_id(id("order-18"));
    assert!(matches!(
        shop.engine.request(&unready),
        Err(CallError::Refused(Refusal::NotReady { .. }))
    ));
    unchanged(&shop, &reserved);
    *shop.backends.version.lock().unwrap() = Some("1".to_owned());
    // An executor that only reads the store, with the same ledger.
    let mut registry = Registry::new();
    registry.register(retouch(&shop.world.runs)).unwrap();
    let reading = Engine::new(
        registry,
        shop.backends.clone(),
        Arc::new(Reader::open(&shop.world.root(), shop.world.fs.clone()).unwrap()),
        Digest::of_bytes(b"config"),
    )
    .with_ledger(shop.ledger.clone());
    assert_eq!(
        reading.request(&unready).unwrap_err(),
        CallError::Refused(Refusal::ReadOnly)
    );
    unchanged(&shop, &reserved);
    assert_eq!(shop.world.ran(), 1);
}

#[test]
fn a_reservation_stays_when_its_run_fails_and_when_its_run_is_deleted_and_pruned() {
    // What is done with a reservation when its run is over is settlement, which is not
    // built: nothing here releases, charges or removes one. It stays as it was made,
    // and it names its run, so that whatever settles it can find it.
    let world = World::new();
    let store = world.store();
    let ledger = Arc::new(FakeLedger::new());
    let engine =
        engine_with(store.clone(), &world.runs, Priced::new(250_000)).with_ledger(ledger.clone());
    let failing = Request::new(call("hello").argument("fail", Value::text("no paint left")))
        .with_id(id("order-17"));
    let failed = ran(engine.request(&failing));
    assert_eq!(failed.outcome().name(), "error");
    let reserved = ledger.reservations(store.as_ref()).unwrap();
    assert_eq!(reserved.len(), 1);
    assert_eq!(reserved[0].run(), failed.run());
    assert_eq!(reserved[0].micros(), 250_000);
    // The failed run's record says what was reserved for it.
    let generation = store.snapshot().unwrap();
    let record = record::stored(&generation, failed.run().as_str(), |e| store.read(e))
        .unwrap()
        .unwrap();
    assert_eq!(record.spend().reserved_micros, Some(250_000));

    store.delete(&[failed.run().as_str()]).unwrap();
    store.prune().unwrap();
    assert_eq!(ledger.reservations(store.as_ref()).unwrap(), reserved);
    // From the id: the id's record names the run, and the run's id names the
    // reservation.
    let generation = store.snapshot().unwrap();
    let outside = generation.get(&id("order-17").admission_name()).unwrap();
    let admission = Admission::from_bytes(&store.read(outside).unwrap()).unwrap();
    let entry = generation
        .get(&Reservation::name_of(admission.run()))
        .unwrap();
    let found = Reservation::from_bytes(&store.read(entry).unwrap()).unwrap();
    assert_eq!(found, reserved[0]);
    assert_eq!(found.admission(), outside.digest);
    assert_eq!(admission.reserved_micros(), Some(found.micros()));
}

#[test]
fn a_run_whose_process_died_keeps_its_reservation() {
    let world = World::new();
    let ledger = Arc::new(FakeLedger::new());
    let dying = {
        let fs = world.fs.clone();
        paint_with(&world.runs, move |_, _| {
            fs.crash(Lost::Unsynced);
            Ok(Produced::nothing())
        })
    };
    let mut registry = Registry::new();
    registry.register(dying).unwrap();
    let engine = Engine::over(
        registry,
        Priced::new(250_000),
        world.store(),
        Digest::of_bytes(b""),
    )
    .with_ledger(ledger.clone());
    assert!(matches!(
        engine.request(&asked("hello", "order-17")),
        Err(CallError::Store(_))
    ));
    drop(engine);
    let fs: Arc<dyn Fs> = Arc::new(RealFs);
    let store = Arc::new(Store::open(&world.root(), fs).unwrap());
    let reserved = ledger.reservations(store.as_ref()).unwrap();
    assert_eq!(reserved.len(), 1);
    // The run is there, admitted and never ended, and it is the reservation's: it
    // holds the admission the reservation names.
    let generation = store.snapshot().unwrap();
    let run = generation.run(reserved[0].run().as_str()).unwrap();
    assert_eq!(run.entries().len(), 1);
    assert_eq!(
        run.get(ADMITTED_NAME).unwrap().digest,
        reserved[0].admission()
    );
}

// --- a stored reservation is not trusted, and neither is a ledger --------------------------------

#[test]
fn a_stored_reservation_that_is_not_one_is_refused_and_nothing_is_admitted() {
    type Planted = (&'static str, Vec<u8>, &'static str);
    let good = RESERVATION.replace("7000-1", "1-1");
    let cases: Vec<Planted> = vec![
        ("spend/reservations/1-1", b"not json\n".to_vec(), "expected"),
        (
            "spend/reservations/1-2",
            good.clone().into_bytes(),
            "it is the reservation of another run",
        ),
        (
            "spend/reservations/1-1",
            vec![b' '; REQUEST_RECORD_MAX_BYTES as usize + 1],
            "1025 bytes",
        ),
        (
            "spend/reservations/1-1",
            good.replace("reservation/1", "reservation/7").into_bytes(),
            "another kind or version",
        ),
        (
            "spend/reservations/\u{1b}[31m",
            good.clone().into_bytes(),
            "another run",
        ),
    ];
    for (name, bytes, says) in cases {
        let world = World::new();
        let store = world.store();
        let mut publication = store.begin().unwrap();
        publication
            .create(name, store::Kind::Diagnostic, &bytes)
            .unwrap();
        publication.publish().unwrap();
        let ledger = Arc::new(FakeLedger::new());
        let engine = engine_with(store.clone(), &world.runs, Priced::new(250_000))
            .with_ledger(ledger.clone());
        let before = held(&world.root(), world.fs.clone());
        let refused = engine.request(&asked("hello", "order-17")).unwrap_err();
        let CallError::Record { what, why } = &refused else {
            panic!("{name:?}: {refused:?}");
        };
        assert_eq!(*what, "a reservation");
        assert!(why.contains(says), "{name:?}: {why}");
        // The sentence and what `{:?}` prints hold no control character and no path.
        let said = format!("{refused} {refused:?}");
        assert!(!said.chars().any(char::is_control), "{said:?}");
        assert!(!said.contains(world.root().to_str().unwrap()), "{said}");
        assert!(said.len() < 1200, "{}", said.len());
        // Nothing was admitted, and the id is free.
        assert_eq!(held(&world.root(), world.fs.clone()), before);
        assert_eq!(world.ran(), 0);
        assert_eq!(ledger.reservations(store.as_ref()).unwrap_err(), refused);
        // One that is too large is refused by its length in the store's entry: its
        // content is never read.
        if says == "1025 bytes" {
            let generation = store.snapshot().unwrap();
            assert_eq!(reservations(&generation, |_| panic!("read")), Err(refused));
        }
    }
    // A name beside the prefix is no reservation, and is not read.
    let world = World::new();
    let store = world.store();
    let mut publication = store.begin().unwrap();
    for name in ["spend/reservation", "spend/reservationz/1-1", "spend0"] {
        publication
            .create(name, store::Kind::Diagnostic, b"not one")
            .unwrap();
    }
    publication.publish().unwrap();
    let generation = store.snapshot().unwrap();
    assert_eq!(reservations(&generation, |_| panic!("read")), Ok(vec![]));
}

/// A ledger that does what the test says with the step.
struct Scripted<F>(F);

impl<F> Ledger for Scripted<F>
where
    F: Fn(&Reservation, &mut Step<'_, '_>) -> Result<(), Declined> + Send + Sync,
{
    fn reserve(&self, reservation: &Reservation, step: &mut Step<'_, '_>) -> Result<(), Declined> {
        (self.0)(reservation, step)
    }
}

#[test]
fn a_ledger_adds_to_the_step_only_names_of_its_own_and_nothing_secret() {
    let key = made_up_key("");
    // What a ledger may not make, and what the refusal says.
    let long = format!("spend/{}", "x".repeat(200));
    let with_key = format!("spend/{key}");
    let cases: Vec<(&str, Vec<u8>, &str)> = vec![
        (
            "requests/order-17/admitted",
            b"x".to_vec(),
            "is no name for",
        ),
        ("spend/reservations/7000-1", b"x".to_vec(), "is no name for"),
        ("ledger/1", b"x".to_vec(), "is no name for"),
        ("spend/a b", b"x".to_vec(), "is no name for"),
        ("spend/\u{1b}[31m", b"x".to_vec(), "is no name for"),
        (&long, b"x".to_vec(), "is no name for"),
        ("spend/held", b"x".to_vec(), "a name is made once"),
        (&with_key, b"x".to_vec(), "must not be written"),
        ("spend/ok", key.clone().into_bytes(), "must not be written"),
    ];
    for (name, bytes, says) in cases {
        let world = World::new();
        let store = world.store();
        let mut publication = store.begin().unwrap();
        publication
            .create("spend/held", store::Kind::Diagnostic, b"held")
            .unwrap();
        publication.publish().unwrap();
        let ledger = {
            let (name, bytes) = (name.to_owned(), bytes.clone());
            Scripted(move |_: &Reservation, step: &mut Step<'_, '_>| {
                step.make(&name, &bytes)?;
                Ok(())
            })
        };
        let engine = engine_with(store.clone(), &world.runs, Priced::new(250_000))
            .with_secrets(Secrets::new([key.clone()]).unwrap())
            .with_ledger(Arc::new(ledger));
        let before = files(&world.root()).len();
        let refused = engine.request(&asked("hello", "order-17")).unwrap_err();
        let CallError::Ledger { why } = &refused else {
            panic!("{name:?}: {refused:?}");
        };
        assert!(why.contains(says), "{name:?}: {why}");
        let said = format!("{refused} {refused:?}");
        assert!(said.starts_with("the ledger failed: "), "{said}");
        assert!(!said.contains(&key), "{name:?}");
        assert!(!said.chars().any(char::is_control), "{said:?}");
        assert!(said.len() < 1200, "{}", said.len());
        // Nothing was published, and nothing of the attempt is left in the store.
        assert_eq!(store.snapshot().unwrap().number(), 1);
        assert_eq!(files(&world.root()).len(), before, "{name:?}");
        assert_eq!(world.ran(), 0);
    }

    // A ledger that fails says so, and nothing is published; one that adds a record
    // of its own has it published with the admission, as a diagnostic reference.
    let world = World::new();
    let store = world.store();
    let failing = Scripted(|_: &Reservation, _: &mut Step<'_, '_>| {
        Err(Declined::Failed(CallError::Ledger {
            why: "its books do not balance".to_owned(),
        }))
    });
    let engine = engine_with(store.clone(), &world.runs, Priced::new(250_000))
        .with_ledger(Arc::new(failing));
    assert_eq!(
        engine
            .request(&asked("hello", "order-17"))
            .unwrap_err()
            .to_string(),
        "the ledger failed: its books do not balance"
    );
    assert_eq!(held(&world.root(), world.fs.clone()), (0, vec![], vec![]));
    let adding = Scripted(|reservation: &Reservation, step: &mut Step<'_, '_>| {
        assert_eq!(step.generation().number(), 0);
        assert_eq!(step.reserved(), Ok(vec![]));
        step.make(&format!("spend/notes/{}", reservation.run()), b"a note\n")?;
        Ok(())
    });
    let engine =
        engine_with(store.clone(), &world.runs, Priced::new(250_000)).with_ledger(Arc::new(adding));
    let finished = ran(engine.request(&asked("hello", "order-17")));
    let generation = store.snapshot().unwrap();
    let note = generation
        .get(&format!("spend/notes/{}", finished.run()))
        .unwrap();
    assert_eq!(note.kind, store::Kind::Diagnostic);
    assert_eq!(store.read(note).unwrap(), b"a note\n");
}

#[test]
fn a_call_whose_cost_has_no_bound_is_not_admitted_and_the_ledger_is_not_asked() {
    // A sum of bounds that passes the largest amount stops there. That is not an
    // amount to reserve or to add to: an executor that reserves refuses the call, and
    // its ledger is never handed it.
    let shop = shop();
    shop.backends.bound.store(u64::MAX - 5, Ordering::SeqCst);
    let prepared = shop.engine.prepare(&shop.call(shop.photo)).unwrap();
    assert!(prepared.estimate().is_unbounded());
    assert!(!Estimate::of_micros(u64::MAX - 1).is_unbounded());
    let before = shop.state();
    let request = Request::new(shop.call(shop.photo))
        .expecting(&prepared)
        .with_id(id("order-17"));
    let refused = shop.engine.request(&request).unwrap_err();
    assert_eq!(
        refused,
        CallError::Refused(Refusal::NoBound {
            action: "retouch".to_owned()
        })
    );
    assert_eq!(
        refused.to_string(),
        "retouch was not admitted: what it may cost has no bound that can be reserved"
    );
    assert_eq!((shop.ledger.asked(), shop.reserved()), (0, vec![]));
    assert_eq!((shop.state(), shop.world.ran()), (before, 0));
    // One backend alone stating the largest amount is the same.
    shop.backends.bound.store(u64::MAX, Ordering::SeqCst);
    assert!(matches!(
        shop.engine.submit(&shop.call(shop.photo)),
        Err(CallError::Refused(Refusal::NoBound { .. }))
    ));
    assert_eq!(shop.ledger.asked(), 0);
    // The fake ledger does not add past the largest amount either: with all but one
    // millionth reserved, a call for two is declined for the one that is left.
    shop.ledger.allow(Some(u64::MAX));
    shop.backends
        .bound
        .store((u64::MAX - 1) / 2, Ordering::SeqCst);
    shop.engine.submit(&shop.call(shop.photo)).unwrap();
    shop.backends.bound.store(1, Ordering::SeqCst);
    let refused = shop.engine.submit(&shop.call(shop.photo)).unwrap_err();
    assert_eq!(
        refused.refusal(),
        Some(&Refusal::OverAllowance {
            action: "retouch".to_owned(),
            asked_micros: 2,
            left_micros: 1,
        })
    );
}

#[test]
fn what_is_reserved_is_the_estimate_of_the_preparation_made_at_admission() {
    // The preparation a request comes with is compared and no more. A lower bound now
    // is a stale preparation like a higher one; and a request that came with none
    // reserves what the backends state when it is admitted.
    let shop = shop();
    let prepared = shop.engine.prepare(&shop.call(shop.photo)).unwrap();
    shop.backends.bound.store(100_000, Ordering::SeqCst);
    let request = Request::new(shop.call(shop.photo)).expecting(&prepared);
    shop.refused_as_stale(request, Stale::Estimate);
    shop.engine.submit(&shop.call(shop.photo)).unwrap();
    let reserved = shop.reserved();
    assert_eq!(reserved.len(), 1);
    assert_eq!(reserved[0].micros(), 200_000);
    // The bound a backend states afterwards changes nothing that was recorded.
    shop.backends.bound.store(900_000, Ordering::SeqCst);
    assert_eq!(shop.reserved(), reserved);
}

#[test]
fn a_run_whose_closing_was_refused_before_it_was_visible_is_still_this_writers() {
    // The end's publication calls `close_run` and is then refused at the generation's
    // file, before the pointer is renamed. The run was not closed: the store still
    // counts it among those this writer made, and the same request again is told so.
    let world = World::new();
    let store = world.store();
    let failing = {
        let fs = world.fs.clone();
        paint_with(&world.runs, move |_, _| {
            fs.refuse_matching(
                0,
                |event| matches!(event, Event::Write(path) if path.ends_with("generation.json")),
            );
            Ok(Produced::nothing())
        })
    };
    let engine = engine(store.clone(), vec![failing]);
    assert!(matches!(
        engine.request(&asked("hello", "order-17")),
        Err(CallError::Store(_))
    ));
    let earlier = repeat(engine.request(&asked("hello", "order-17")));
    // Abandoned (ADR 0008, "A run with nothing behind it"): open here, and nothing ends it.
    assert!(
        matches!(earlier.state(), RunState::Abandoned { failure: Some(_) }),
        "{:?}",
        earlier.state()
    );
    assert!(store.made_here(earlier.run().as_str()));
    assert_eq!(world.ran(), 1);
}

#[test]
fn a_preparation_handed_to_a_call_that_is_refused_now_is_stale_under_the_calls_action() {
    // The refusal names the action that was called, not the one the preparation is of.
    let shop = shop();
    let prepared = shop.engine.prepare(&shop.call(shop.photo)).unwrap();
    let request = Request::new(Call::new("vanish")).expecting(&prepared);
    assert_eq!(
        shop.engine.request(&request).unwrap_err(),
        CallError::Refused(Refusal::Stale {
            action: "vanish".to_owned(),
            why: Stale::Refused(Box::new(Refusal::UnknownAction {
                name: "vanish".to_owned()
            })),
        })
    );
    assert_eq!((shop.ledger.asked(), shop.world.ran()), (0, 0));
}

// --- an id that is taken between the look and a refusal ------------------------------------------

type Once = Box<dyn FnOnce() + Send>;

/// A clock that runs a hook the first time it is asked: admission asks the time after
/// the look for the id and the preparation, and before the ledger is asked.
struct Hooked(Mutex<Option<Once>>);

impl Clock for Hooked {
    fn now_ms(&self) -> u64 {
        let hook = self.0.lock().unwrap().take();
        if let Some(hook) = hook {
            hook();
        }
        7000
    }
}

/// Backends that run a hook the first time they are asked, and then offer nothing: a
/// call that is prepared is not ready.
struct Withdrawn(Mutex<Option<Once>>);

impl Backends for Withdrawn {
    fn resolve(
        &self,
        _: Capability,
        _: Option<&str>,
        _: &[String],
    ) -> Result<ModelIdentity, Unavailable> {
        let hook = self.0.lock().unwrap().take();
        if let Some(hook) = hook {
            hook();
        }
        Err(Unavailable::new("the filler was taken away"))
    }
}

#[test]
fn a_refusal_of_a_request_whose_id_was_taken_meanwhile_is_answered_from_the_id() {
    // A request looks its id up and finds nothing. Before it is admitted, a request
    // under the same id is, and takes the last of the allowance. The first is then
    // declined by the ledger. What it is told is what the id says: the same request
    // has its run, and another request has a conflict. It is not told that it is over
    // the allowance, with a run of its own request executing and reserved.
    let race = |text: &'static str| {
        let world = World::new();
        let store = world.store();
        let ledger = Arc::new(FakeLedger::with_allowance(400_000));
        let priced = |store: &Arc<Store>| {
            engine_with(store.clone(), &world.runs, Priced::new(250_000))
                .with_ledger(ledger.clone())
        };
        let winner = priced(&store);
        let first = Arc::new(Mutex::new(None));
        let hook: Once = {
            let first = first.clone();
            Box::new(move || {
                *first.lock().unwrap() = Some(winner.request(&asked("hello", "order-17")));
            })
        };
        let loser = priced(&store).with_clock(Arc::new(Hooked(Mutex::new(Some(hook)))));
        let answer = loser.request(&asked(text, "order-17"));
        let first = ran(first.lock().unwrap().take().unwrap());
        assert_eq!(ledger.reservations(store.as_ref()).unwrap().len(), 1);
        assert_eq!(world.ran(), 1);
        (answer, first)
    };
    let (answer, first) = race("hello");
    let earlier = repeat(answer);
    assert_eq!(earlier.run(), first.run());
    assert_eq!(earlier.state(), &RunState::Ended(completed()));
    let (answer, first) = race("goodbye");
    assert_eq!(
        answer.unwrap_err(),
        CallError::Refused(Refusal::Conflict {
            request: id("order-17"),
            run: first.run().clone(),
        })
    );

    // The same for a request that is found not ready after its id was taken.
    let world = World::new();
    let store = world.store();
    let winner = world.engine(store.clone());
    let first = Arc::new(Mutex::new(None));
    let hook: Once = {
        let first = first.clone();
        Box::new(move || {
            *first.lock().unwrap() = Some(winner.request(&asked("hello", "order-17")));
        })
    };
    let withdrawn = Arc::new(Withdrawn(Mutex::new(Some(hook))));
    let loser = engine_with(store.clone(), &world.runs, withdrawn);
    let earlier = repeat(loser.request(&asked("hello", "order-17")));
    let first = ran(first.lock().unwrap().take().unwrap());
    assert_eq!(earlier.run(), first.run());
    // An id that nobody took leaves the refusal as it is.
    assert!(matches!(
        loser.request(&asked("hello", "order-18")),
        Err(CallError::Refused(Refusal::NotReady { .. }))
    ));
    assert_eq!(world.ran(), 1);
}

#[test]
fn a_run_is_not_named_with_an_id_whose_reservation_is_still_there() {
    // Two thousand reservations left by runs that were deleted, under the ids the
    // executor would choose next. It passes over each when it names the run: none of
    // them costs one of the eight namings, and the ledger is asked once.
    let world = World::new();
    let store = world.store();
    let mut publication = store.begin().unwrap();
    for count in 1..=2000 {
        let name = format!("spend/reservations/7000-{count}");
        publication
            .create(&name, store::Kind::Diagnostic, b"left behind\n")
            .unwrap();
    }
    publication.publish().unwrap();
    let asked_times = Arc::new(AtomicU64::new(0));
    let counting = {
        let asked_times = asked_times.clone();
        Scripted(move |_: &Reservation, _: &mut Step<'_, '_>| {
            asked_times.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    };
    let engine = engine_with(store.clone(), &world.runs, Priced::new(250_000))
        .with_ledger(Arc::new(counting));
    let finished = ran(engine.request(&asked("hello", "order-17")));
    assert_eq!(asked_times.load(Ordering::SeqCst), 1);
    let count: u64 = finished.run().as_str()["7000-".len()..].parse().unwrap();
    assert!(count > 2000, "{count}");
    assert_eq!(store.snapshot().unwrap().runs().len(), 1);
}

#[test]
fn a_ledgers_name_is_two_hundred_characters_at_most_and_the_fake_does_not_add_past_the_largest() {
    let made = |length: usize| {
        let world = World::new();
        let store = world.store();
        let name = format!("spend/{}", "x".repeat(length - "spend/".len()));
        assert_eq!(name.len(), length);
        let ledger = Scripted(move |_: &Reservation, step: &mut Step<'_, '_>| {
            step.make(&name, b"x")?;
            Ok(())
        });
        let engine =
            engine_with(store.clone(), &world.runs, Priced::new(1)).with_ledger(Arc::new(ledger));
        engine.request(&asked("hello", "order-17"))
    };
    ran(made(200));
    assert!(matches!(made(201), Err(CallError::Ledger { .. })));

    // Two reservations of one under the largest amount, made with no limit. Their sum
    // is past the largest amount: it stops there, and under an allowance of the
    // largest amount nothing is left, not what a sum that wrapped would leave.
    let world = World::new();
    let store = world.store();
    let ledger = Arc::new(FakeLedger::new());
    let backends = Priced::new(u64::MAX - 1);
    let engine =
        engine_with(store.clone(), &world.runs, backends.clone()).with_ledger(ledger.clone());
    engine.submit(&call("one")).unwrap();
    engine.submit(&call("two")).unwrap();
    ledger.allow(Some(u64::MAX));
    backends.bound.store(1, Ordering::SeqCst);
    assert_eq!(
        engine.submit(&call("three")).unwrap_err().refusal(),
        Some(&Refusal::OverAllowance {
            action: "paint".to_owned(),
            asked_micros: 1,
            left_micros: 0,
        })
    );
}
