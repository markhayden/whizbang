//! The binary wired (plan 1B, task W1): the executor as `wiring::runner` builds it,
//! over a config file and an env file the test writes into its own scratch folder,
//! with a fake kind of backend registered by the test, and the real binary reading
//! what it left. Every key here is made up at run time.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use whizbang::wiring::{self, Over};
use whizbang_core::capability::{Declaration, FillRequest, Picture, Price, Support};
use whizbang_core::config::{self, Config, KindRefused, Kinds, Setting, Table};
use whizbang_core::contract::{
    Action, ActionDescriptor, Call, CallError, Capability, Clock, Context, Engine, Executor,
    Failure, Kind, Made, ModelIdentity, PreparedCall, Produced, Readiness, Refusal, RefusalKind,
    Registry, Request, Requirement, Secrets, Value,
};
use whizbang_core::egress::Recipient;
use whizbang_core::identity::CanonicalPixels;
use whizbang_core::spend::{AGE_PART, CAP_PART, OVER_THE_CAP, PREPARATION_HOLDS_MS, TOO_OLD};
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Reader, Store};
use whizbang_testkit::capabilities::{FakeFiller, Paint};
use whizbang_testkit::echo::Echo;
use whizbang_testkit::scratch::Scratch;

/// The digest of the empty config, as ADR 0011 fixes it ("The digest").
const EMPTY_CONFIG: &str =
    "sha256:9d60a543ea39be1ca1bc2c4e1a90094a59ded6bdcfdf1c21436f4026f207c4a6";

/// A made-up key, long enough to be a secret, assembled here so that no scan of this
/// file finds one.
fn made_up_key(tail: &str) -> String {
    ["made", "up", "key", "0123456789", tail].join("-")
}

/// A kind named `fake`, whose backends are fake fillers that send nothing. A backend
/// of it declares the model its table gives, of the backend `fake` at the version
/// `1`; names the origin of its table's address as its recipient, when the table has
/// one; and states the price its setting `at_most_micros` gives, when it gives one.
/// It keeps each filler it made.
#[derive(Default)]
pub struct Fakes {
    made: Mutex<Vec<Arc<FakeFiller>>>,
}

impl Fakes {
    /// How many requests the backends it made were sent, over all of them.
    pub fn requests(&self) -> u64 {
        let made = self.made.lock().unwrap();
        made.iter().map(|filler| filler.requests()).sum()
    }
}

/// A fake kind is shared with the test that registered it.
struct Shared(Arc<Fakes>);

impl config::Kind for Shared {
    fn name(&self) -> &str {
        "fake"
    }

    fn make(&self, table: &Table<'_>) -> Result<Made, KindRefused> {
        let model = table.backend().model().unwrap_or("m");
        let mut declaration = Declaration::new(ModelIdentity::new("fake", model, "1"), &[])
            .map_err(|_| KindRefused::Table("a fake backend cannot declare that model"))?;
        if let Some(address) = table.backend().address() {
            declaration = declaration.sending_to(Recipient::new(address.origin().clone()));
        }
        for (key, setting) in table.backend().settings() {
            let ("at_most_micros", Setting::Integer(micros)) = (key, setting) else {
                return Err(KindRefused::Table("a fake backend takes at_most_micros"));
            };
            #[allow(clippy::cast_precision_loss)]
            let price = Price::at_most(*micros as f64)
                .map_err(|_| KindRefused::Table("a fake backend cannot state that price"))?;
            declaration = declaration.priced(price);
        }
        let filler = Arc::new(FakeFiller::new(Paint::Inverse).declared_as(declaration));
        self.0.made.lock().unwrap().push(filler.clone());
        Ok(Made::Fill(filler))
    }
}

/// The kinds of this build with the fake kind registered beside them.
pub fn kinds_with(fakes: &Arc<Fakes>) -> Kinds {
    let mut kinds = wiring::kinds();
    kinds.register(Arc::new(Shared(fakes.clone()))).unwrap();
    kinds
}

type Body = Box<dyn Fn(&PreparedCall, &Context) -> Result<Produced, Failure> + Send + Sync>;

struct Scripted {
    descriptor: ActionDescriptor,
    runs: Arc<AtomicU64>,
    body: Body,
}

impl Action for Scripted {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        self.runs.fetch_add(1, Ordering::SeqCst);
        (self.body)(call, context)
    }
}

/// A request a filler takes: a small picture and the part of it to fill.
pub fn fill_request() -> FillRequest {
    let rgba = (0..24u8).flat_map(|at| [at, at, at, 255]).collect();
    let picture = Picture::new(CanonicalPixels::from_rgba(6, 4, rgba).unwrap()).unwrap();
    let mut plane = vec![0; 24];
    plane[..6].fill(255);
    FillRequest::new(picture, Support::from_plane(6, 4, plane).unwrap(), None).unwrap()
}

/// `paints`: an action that needs a filler and asks it once. It fails, as an error of
/// the kind `fill`, when the filler answers with a fault. It counts its runs.
pub fn paints(runs: &Arc<AtomicU64>) -> Arc<dyn Action> {
    let draft = ActionDescriptor::draft("paints", "1.0.0", Kind::Primitive, "A test action.");
    let descriptor = draft.needs(Requirement::new(Capability::Fill)).build();
    Arc::new(Scripted {
        descriptor: descriptor.unwrap(),
        runs: runs.clone(),
        body: Box::new(|_, context| {
            let fill = context.capabilities().fill().expect("a filler is handed");
            match fill.fill(&fill_request()) {
                Ok(_) => Ok(Produced::nothing()),
                Err(fault) => Err(Failure::error("fill", &fault.to_string())),
            }
        }),
    })
}

/// A scratch folder with a store in it, a place for a config file and an env file,
/// the fake kind, and a count of the runs of `paints`.
pub struct World {
    dir: Scratch,
    pub root: PathBuf,
    pub fakes: Arc<Fakes>,
    pub runs: Arc<AtomicU64>,
}

impl World {
    pub fn new(label: &str) -> World {
        let dir = Scratch::new(label);
        let root = dir.path().join("store");
        drop(Store::open(&root, Arc::new(RealFs)).unwrap());
        World {
            dir,
            root,
            fakes: Arc::new(Fakes::default()),
            runs: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn config_path(&self) -> PathBuf {
        self.dir.path().join("whizbang.toml")
    }

    pub fn env_path(&self) -> PathBuf {
        self.dir.path().join("keys.env")
    }

    /// Writes the config file, and answers what a command that names it is built
    /// over: that file, no env file, and the kinds with the fake.
    pub fn config(&self, text: &str) -> Over {
        std::fs::write(self.config_path(), text).unwrap();
        Over::nothing()
            .config(Some(&self.config_path()))
            .kinds(kinds_with(&self.fakes))
    }

    /// The actions of this build with `paints` and `echo` registered beside them.
    pub fn registry(&self) -> Registry {
        let mut registry = wiring::registry().unwrap();
        registry.register(paints(&self.runs)).unwrap();
        registry.register(Arc::new(Echo::new())).unwrap();
        registry
    }

    /// The binary's executor over this world's store, built over this. The store is
    /// held to write for as long as the executor lives.
    pub fn runner(&self, over: &Over) -> Engine {
        let store = Arc::new(Store::open(&self.root, Arc::new(RealFs)).unwrap());
        wiring::runner(self.registry(), store, over).expect("the executor builds")
    }

    pub fn ran(&self) -> u64 {
        self.runs.load(Ordering::SeqCst)
    }

    /// Each run the store holds, with its names.
    pub fn held(&self) -> Vec<(String, Vec<String>)> {
        let reader = Reader::open(&self.root, Arc::new(RealFs)).unwrap();
        let generation = reader.snapshot().unwrap();
        let runs = generation.runs().iter();
        runs.map(|(id, run)| (id.clone(), run.entries().keys().cloned().collect()))
            .collect()
    }

    /// Every file under the scratch folder's store, as bytes.
    pub fn bytes(&self) -> Vec<Vec<u8>> {
        fn walk(dir: &Path, found: &mut Vec<Vec<u8>>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, found);
                } else {
                    found.push(std::fs::read(&path).unwrap());
                }
            }
        }
        let mut found = Vec::new();
        walk(&self.root, &mut found);
        found
    }
}

pub struct Ran {
    pub code: Option<i32>,
    pub out: String,
    pub err: String,
}

/// The real binary, with these arguments and then `--store` and the store.
pub fn whizbang(args: &[&str], root: &Path) -> Ran {
    let out = Command::new(env!("CARGO_BIN_EXE_whizbang"))
        .args(args)
        .arg("--store")
        .arg(root)
        .output()
        .expect("the binary runs");
    Ran {
        code: out.status.code(),
        out: String::from_utf8(out.stdout).expect("output is UTF-8"),
        err: String::from_utf8(out.stderr).expect("output is UTF-8"),
    }
}

/// What `runs show` prints of this run.
pub fn shown(root: &Path, run: &str) -> String {
    let shown = whizbang(&["runs", "show", run], root);
    assert_eq!(shown.code, Some(0), "{}", shown.err);
    shown.out
}

pub fn has_line(text: &str, line: &str) {
    assert!(
        text.lines().any(|shown| shown == line),
        "{line:?} in:\n{text}"
    );
}

fn not_ready(refused: Result<impl std::fmt::Debug, CallError>) -> String {
    match refused {
        Err(CallError::Refused(Refusal::NotReady { action, missing })) => {
            assert_eq!(action, "paints");
            assert_eq!(missing.len(), 1, "{missing:?}");
            assert_eq!(missing[0].capability().name(), "fill");
            missing[0].why().to_owned()
        }
        other => panic!("not refused as not ready: {other:?}"),
    }
}

// --- the snapshot path -------------------------------------------------------------------------

#[test]
fn with_no_config_the_binarys_executor_is_under_the_empty_config_and_a_call_with_no_backend_to_hand_is_refused_as_not_ready_with_no_run()
 {
    // A command that names no config runs under the empty config (ADR 0011, "Where
    // the file is"): its digest is that config's, not the digest of no bytes, and the
    // executor asks a snapshot of it and nothing else.
    let world = World::new("wired-empty");
    let executor = world.runner(&Over::nothing());
    assert_eq!(Config::empty().digest().to_string(), EMPTY_CONFIG);
    let prepared = executor.prepare(&Call::new("paints")).unwrap();
    assert_eq!(prepared.config().to_string(), EMPTY_CONFIG);
    assert!(matches!(prepared.readiness(), Readiness::NotReady(_)));
    // No backend is there to hand: the call is refused as not ready, before
    // admission. No run is made and the action does not run.
    let why = not_ready(executor.submit(&Call::new("paints")));
    assert!(why.contains("fill"), "{why}");
    assert_eq!(world.ran(), 0);
    assert!(world.held().is_empty(), "{:?}", world.held());
    // A call that needs no backend runs, and its record names the empty config.
    let echoed = executor
        .submit(&Call::new("echo").argument("text", Value::text("hello")))
        .unwrap();
    drop(executor);
    let text = shown(&world.root, echoed.run().as_str());
    has_line(&text, &format!("config: {EMPTY_CONFIG}"));
    has_line(&text, "settings: none");
}

#[test]
fn a_backend_of_a_kind_this_build_does_not_have_is_not_ready_through_the_binarys_wiring_and_says_why()
 {
    // The build registers no kind (`wiring::kinds`), so a backend a config names is
    // offered and is not ready, with the reason; nothing answers in its place.
    let world = World::new("wired-no-kind");
    let config = "[backends.fill]\nkind = \"fake\"\nmodel = \"paint-1\"\n";
    std::fs::write(world.config_path(), config).unwrap();
    assert!(wiring::kinds().is_empty());
    let over = Over::nothing()
        .config(Some(&world.config_path()))
        .kinds(wiring::kinds());
    let executor = world.runner(&over);
    let why = not_ready(executor.submit(&Call::new("paints")));
    assert_eq!(
        why,
        "the default fill backend is not ready: its kind is not one this build has"
    );
    assert_eq!(world.ran(), 0);
    assert!(world.held().is_empty());
}

#[test]
fn under_a_config_a_run_is_handed_the_backend_its_kind_made_and_runs_show_prints_the_configs_digest_the_settings_and_the_model()
 {
    let world = World::new("wired-config");
    let text = "[backends.fill]\nkind = \"fake\"\nmodel = \"paint-1\"\n";
    let over = world.config(text);
    let digest = Config::parse(text.as_bytes(), &Secrets::none())
        .unwrap()
        .digest();
    assert_ne!(digest.to_string(), EMPTY_CONFIG);
    let executor = world.runner(&over);
    let finished = executor.submit(&Call::new("paints")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    // The backend the kind made of the config's table answered, once.
    assert_eq!((world.ran(), world.fakes.requests()), (1, 1));
    drop(executor);
    let text = shown(&world.root, finished.run().as_str());
    has_line(&text, &format!("config: {digest}"));
    has_line(&text, "settings: 2");
    has_line(&text, "  backends.fill.default.kind = \"fake\"");
    has_line(&text, "  backends.fill.default.model = \"paint-1\"");
    has_line(&text, "models: 1");
    assert!(
        text.contains("  fill\tfake paint-1 1\tanswered\t"),
        "{text}"
    );
    // A config that is changed is read again by the next call: the same executor
    // prepares under the new digest.
    let executor = world.runner(&over);
    let before = executor.prepare(&Call::new("paints")).unwrap().config();
    assert_eq!(before, digest);
    std::fs::write(
        world.config_path(),
        "[backends.fill]\nkind = \"fake\"\nmodel = \"paint-2\"\n",
    )
    .unwrap();
    let after = executor.prepare(&Call::new("paints")).unwrap().config();
    assert_ne!(after, digest);
}

// --- the parts of the admission ----------------------------------------------------------------

/// A config whose one filler states a bound of a quarter of a dollar for a request,
/// with this cap on one call.
fn capped(cap_usd: &str) -> String {
    format!(
        "[backends.fill]\nkind = \"fake\"\n\n[backends.fill.settings]\nat_most_micros = 250000\n\n\
         [spend]\ncall_cap_usd = {cap_usd}\n"
    )
}

#[test]
fn a_call_over_the_cap_of_the_binarys_config_is_refused_before_a_run_exists() {
    let world = World::new("wired-cap");
    let over = world.config(&capped("0.10"));
    let executor = world.runner(&over);
    let refused = executor.submit(&Call::new("paints"));
    let Err(CallError::Refused(Refusal::ByPart {
        action,
        part,
        kind,
        why,
    })) = refused
    else {
        panic!("not refused by the cap: {refused:?}");
    };
    assert_eq!((action.as_str(), part), ("paints", CAP_PART));
    assert_eq!(kind, RefusalKind::OverAllowance);
    assert_eq!(why, OVER_THE_CAP);
    // Before a run exists: nothing is in the store, and nothing ran or was sent.
    assert!(world.held().is_empty(), "{:?}", world.held());
    assert_eq!((world.ran(), world.fakes.requests()), (0, 0));
    // The cap raised in the file is the cap the next call is held to.
    std::fs::write(world.config_path(), capped("0.25")).unwrap();
    let prepared = executor.prepare(&Call::new("paints")).unwrap();
    assert_eq!(prepared.readiness(), &Readiness::Ready);
}

/// A clock a test moves by hand.
struct Hand(AtomicU64);

impl Clock for Hand {
    fn now_ms(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}

#[test]
fn a_request_that_comes_with_a_preparation_an_hour_old_is_refused_as_stale_by_the_binarys_executor()
{
    let world = World::new("wired-age");
    let clock = Arc::new(Hand(AtomicU64::new(1_000_000)));
    // The binary's executor, telling the time by the test's clock.
    let executor = world.runner(&Over::nothing()).with_clock(clock.clone());
    let call = Call::new("echo").argument("text", Value::text("hello"));
    let prepared = executor.prepare(&call).unwrap();
    clock.0.fetch_add(PREPARATION_HOLDS_MS, Ordering::SeqCst);
    let refused = executor.request(&Request::new(call.clone()).expecting(&prepared));
    let Err(CallError::Refused(Refusal::ByPart {
        part, kind, why, ..
    })) = refused
    else {
        panic!("not refused for its age: {refused:?}");
    };
    assert_eq!(part, AGE_PART);
    assert_eq!(kind, RefusalKind::Stale);
    assert_eq!(why, TOO_OLD);
    assert!(world.held().is_empty());
    // One millisecond younger, it is admitted.
    clock.0.fetch_sub(1, Ordering::SeqCst);
    executor
        .request(&Request::new(call).expecting(&prepared))
        .unwrap();
    assert_eq!(world.held().len(), 1);
}

#[test]
fn the_parts_the_binary_is_wired_with_add_no_record_to_an_admission() {
    // An admission may hold 16 records of 64 KiB each from its parts (ADR 0015, "A
    // part in the admission"), and a tree of runs that much for each run. The two
    // parts of this build, the cap's and the age's, write none: a run's names hold
    // nothing under `admission/`, with a cap configured and a preparation shown.
    let world = World::new("wired-parts");
    let over = world.config(&capped("0.25"));
    let executor = world.runner(&over);
    let prepared = executor.prepare(&Call::new("paints")).unwrap();
    executor
        .request(&Request::new(Call::new("paints")).expecting(&prepared))
        .unwrap();
    assert_eq!(world.ran(), 1);
    let held = world.held();
    assert_eq!(held.len(), 1);
    let names = &held[0].1;
    assert!(names.contains(&"admitted".to_owned()), "{names:?}");
    assert!(
        !names.iter().any(|name| name.starts_with("admission/")),
        "{names:?}"
    );
}

// --- the secrets -------------------------------------------------------------------------------

#[test]
fn the_binarys_source_and_its_executor_are_told_of_the_same_secrets() {
    let world = World::new("wired-secrets");
    let key = made_up_key("told");
    let secrets = Secrets::new([key.clone()]).unwrap();
    // The executor takes the secret out of what it writes: a diagnostic, a journal
    // and a record that an action filled with it.
    let over = world
        .config("[backends.fill]\nkind = \"fake\"\n")
        .knowing(secrets);
    let executor = world.runner(&over);
    let said = format!("the key is {key}");
    executor
        .submit(&Call::new("echo").argument("text", Value::text(&said)))
        .unwrap();
    assert_eq!(world.held().len(), 1);
    for bytes in world.bytes() {
        let found = bytes.windows(key.len()).any(|at| at == key.as_bytes());
        assert!(!found, "a file of the store holds the key");
    }
    // The source knows it too: a config that holds it is refused whole, in fixed
    // words, and no run is made of the call.
    let pasted = format!("[backends.fill]\nkind = \"fake\"\nmodel = \"{key}\"\n");
    std::fs::write(world.config_path(), pasted).unwrap();
    let refused = executor.submit(&Call::new("echo").argument("text", Value::text("hello")));
    let Err(CallError::Config { why }) = refused else {
        panic!("the config was used: {refused:?}");
    };
    assert!(why.starts_with("the config holds a secret"), "{why}");
    assert!(!why.contains(&key));
    assert_eq!(world.held().len(), 1);
    // An executor that was told of no secret reads that config: the two were told
    // the same, which is nothing.
    drop(executor);
    let untold = world.runner(&over.clone().knowing(Secrets::none()));
    let prepared = untold.prepare(&Call::new("paints")).unwrap();
    assert_eq!(prepared.readiness(), &Readiness::Ready);
}

#[test]
fn a_key_of_the_env_file_is_taken_out_of_what_a_run_under_the_binarys_executor_writes() {
    // The env file named beside the config holds the value of the variable the
    // config names: it is the executor's secret from the first call on.
    let world = World::new("wired-env");
    let key = made_up_key("env");
    std::fs::write(world.env_path(), format!("FAKE_KEY={key}\n")).unwrap();
    let text = "[backends.fill]\nkind = \"fake\"\nkey_env = \"FAKE_KEY\"\n";
    let over = world.config(text).env_file(Some(&world.env_path()));
    let executor = world.runner(&over);
    let said = format!("the key is {key}");
    executor
        .submit(&Call::new("echo").argument("text", Value::text(&said)))
        .unwrap();
    for bytes in world.bytes() {
        let found = bytes.windows(key.len()).any(|at| at == key.as_bytes());
        assert!(!found, "a file of the store holds the key");
    }
    // Without the env file the config, which names a variable, is not used.
    drop(executor);
    let executor = world.runner(&world.config(text));
    let refused = executor.submit(&Call::new("echo").argument("text", Value::text("hello")));
    let Err(CallError::Config { why }) = refused else {
        panic!("the config was used: {refused:?}");
    };
    assert!(why.contains("no env file was given"), "{why}");
}

// --- the policy --------------------------------------------------------------------------------

#[test]
fn a_recipient_the_config_does_not_approve_is_refused_at_the_gate_and_one_its_egress_table_approves_is_let_through()
 {
    // A backend with an address names that origin as its recipient. The policy is
    // the config's `egress` table and nothing else: with no table nothing is
    // approved, and the request is refused at the gate's place, before its backend
    // is called.
    let world = World::new("wired-egress");
    let backend = "[backends.fill]\nkind = \"fake\"\nurl = \"https://api.example.com/v1\"\n";
    let executor = world.runner(&world.config(backend));
    let refused = executor.submit(&Call::new("paints")).unwrap();
    assert_eq!(refused.outcome().name(), "error");
    assert_eq!((world.ran(), world.fakes.requests()), (1, 0));
    drop(executor);
    let text = shown(&world.root, refused.run().as_str());
    has_line(&text, "egress: 1");
    assert!(
        text.contains("  https://api.example.com\tnone\t") && text.contains("\trefused\n"),
        "{text}"
    );
    assert!(text.contains("\trequest_not_approved\t"), "{text}");
    // Approved for the class a fill with no prompt is said to hold, which is none
    // known: a recipient that may receive some class is let through, and its
    // backend is called.
    let approved = format!(
        "{backend}\n[egress.recipients.api]\norigin = \"https://api.example.com\"\n\
         classes = [\"source_images\"]\n"
    );
    let executor = world.runner(&world.config(&approved));
    let let_through = executor.submit(&Call::new("paints")).unwrap();
    assert_eq!(let_through.outcome().name(), "completed");
    assert_eq!((world.ran(), world.fakes.requests()), (2, 1));
    drop(executor);
    let text = shown(&world.root, let_through.run().as_str());
    assert!(!text.contains("request_not_approved"), "{text}");
}
