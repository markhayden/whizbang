//! The binary wired (plan 1B, task W1): the executor as `wiring::runner` builds it,
//! over a config file and an env file the test writes into its own scratch folder,
//! with a fake kind of backend registered by the test, and the real binary reading
//! what it left. Every key here is made up at run time.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use whizbang::pack::OneWorker;
use whizbang::wiring::{self, Models, Over, Said, Uses};
use whizbang_backends::models::{
    Catalogue, CatalogueFile, Entry, EntryParts, FileName, Installations, Licence, ModelName,
    ModelsDir, Platform, Revision, Title, install,
};
use whizbang_core::capability::{
    Bounds, Declaration, Fault, FillRequest, Filler, Installed, Inventory, Painted, Picture, Price,
    Support, Unready,
};
use whizbang_core::config::{self, Config, KindRefused, Kinds, Setting, Table};
use whizbang_core::contract::{
    Action, ActionDescriptor, Answer, Argument, Call, CallError, Cancellation, Capability, Clock,
    Context, Engine, Executor, Failure, Kind, Made, ModelIdentity, PreparedCall, Produced,
    Readiness, Refusal, RefusalKind, Registry, Request, Requirement, Secrets, Value, ValueType,
};
use whizbang_core::egress::{Address, DataClass, Origin, Piece, Recipient};
use whizbang_core::identity::{CanonicalPixels, Digest};
use whizbang_core::spend::{
    AGE_PART, CAP_PART, OVER_THE_CAP, PREPARATION_HOLDS_MS, Record, TOO_OLD, TurnBy,
};
use whizbang_core::store::fs::{Fs, Held, LockFile, NewFile, OpenFile, RealFs};
use whizbang_core::store::{Reader, Store};
use whizbang_testkit::capabilities::{FakeFiller, Paint};
use whizbang_testkit::downloads::FakeDownloader;
use whizbang_testkit::echo::Echo;
use whizbang_testkit::fs_shim::ShimFs;
use whizbang_testkit::scratch::Scratch;

/// The digest of the empty config, as ADR 0011 fixes it ("The digest").
const EMPTY_CONFIG: &str =
    "sha256:9d60a543ea39be1ca1bc2c4e1a90094a59ded6bdcfdf1c21436f4026f207c4a6";

/// A made-up key, long enough to be a secret, assembled here so that no scan of this
/// file finds one.
fn made_up_key(tail: &str) -> String {
    ["made", "up", "key", "0123456789", tail].join("-")
}

/// The model a backend of the fake kind declares when its table names none: the one
/// model the test's catalogue holds, installed in every [`World`].
pub const MODEL: &str = "paint-1";

/// A kind named `fake`, whose backends are fake fillers that send nothing. A backend
/// of it declares the model its table gives ([`MODEL`] when it gives none), of the
/// backend `fake` at the version `1`; names the origin of its table's address as its recipient, when the table has
/// one; states the price its setting `at_most_micros` gives, when it gives one; and
/// reports the charge its setting `charges_micros` gives with every answer, when it
/// gives one. It keeps each filler it made.
#[derive(Default)]
pub struct Fakes {
    made: Mutex<Vec<Arc<FakeFiller>>>,
    dies: Mutex<Option<Arc<ShimFs>>>,
    out: Mutex<Option<Looks>>,
    posts: Mutex<Option<Vec<Piece>>>,
}

/// What a test does while a request is with a backend.
type Looks = Arc<dyn Fn() + Send + Sync>;

impl Fakes {
    /// How many requests the backends it made were sent, over all of them.
    pub fn requests(&self) -> u64 {
        let made = self.made.lock().unwrap();
        made.iter().map(|filler| filler.requests()).sum()
    }

    /// From now on, a backend it made that is sent a request has this file system
    /// refuse the next call that changes the disk, and then answers: the first
    /// publication after the request left, which is its settlement, fails.
    pub fn die_once_a_request_has_left(&self, fs: &Arc<ShimFs>) {
        *self.dies.lock().unwrap() = Some(fs.clone());
    }

    /// From now on, a backend it made that is sent a request does this before it
    /// answers: what a test looks at while a request is out.
    pub fn while_a_request_is_out(&self, look: impl Fn() + Send + Sync + 'static) {
        *self.out.lock().unwrap() = Some(Arc::new(look));
    }

    /// From now on, a backend it makes with an address sends these pieces there, with
    /// each request, through the sender its run hands it, before it answers.
    pub fn posting(&self, pieces: Vec<Piece>) {
        *self.posts.lock().unwrap() = Some(pieces);
    }
}

/// A fake filler as the fake kind hands it over: it answers as the filler it holds
/// does, after doing what [`Fakes::die_once_a_request_has_left`] asked for.
struct Handed(Arc<FakeFiller>, Arc<Fakes>);

impl Filler for Handed {
    fn declaration(&self) -> &Declaration {
        self.0.declaration()
    }

    fn fill(&self, request: &FillRequest, bounds: &Bounds) -> Result<Painted, Fault> {
        if let Some(fs) = self.1.dies.lock().unwrap().as_ref() {
            fs.refuse_before(fs.events().len());
        }
        let look = self.1.out.lock().unwrap().clone();
        if let Some(look) = look {
            look();
        }
        self.0.fill(request, bounds)
    }
}

/// A fake kind is shared with the test that registered it.
struct Shared(Arc<Fakes>);

impl config::Kind for Shared {
    fn name(&self) -> &str {
        "fake"
    }

    fn make(&self, table: &Table<'_>) -> Result<Made, KindRefused> {
        let model = table.backend().model().unwrap_or(MODEL);
        let mut declaration = Declaration::new(ModelIdentity::new("fake", model, "1"), &[])
            .map_err(|_| KindRefused::Table("a fake backend cannot declare that model"))?;
        // The setting `upstream`, an origin, is one the backend's recipient forwards to.
        let upstream = table
            .backend()
            .settings()
            .find_map(|(key, setting)| match (key, setting) {
                ("upstream", Setting::Text(origin)) => Origin::of(origin, &Secrets::none()).ok(),
                _ => None,
            });
        if let Some(address) = table.backend().address() {
            let mut recipient = Recipient::new(address.origin().clone());
            if let Some(upstream) = upstream {
                recipient = recipient
                    .forwarding_to(vec![upstream])
                    .map_err(|_| KindRefused::Table("a fake backend cannot forward there"))?;
            }
            declaration = declaration.sending_to(recipient);
        }
        let mut charge = None;
        for (key, setting) in table.backend().settings() {
            match (key, setting) {
                ("upstream", Setting::Text(_)) => {}
                ("at_most_micros", Setting::Integer(micros)) => {
                    #[allow(clippy::cast_precision_loss)]
                    let price = Price::at_most(*micros as f64).map_err(|_| {
                        KindRefused::Table("a fake backend cannot state that price")
                    })?;
                    declaration = declaration.priced(price);
                }
                ("charges_micros", Setting::Integer(micros)) => {
                    charge = u64::try_from(*micros).ok();
                }
                _ => {
                    return Err(KindRefused::Table(
                        "a fake backend takes at_most_micros, charges_micros and upstream",
                    ));
                }
            }
        }
        let mut filler = FakeFiller::new(Paint::Inverse).declared_as(declaration);
        let posts = self.0.posts.lock().unwrap().clone();
        if let (Some(pieces), Some(address)) = (posts, table.backend().address()) {
            filler = filler.posting(address.clone(), pieces);
        }
        let filler = Arc::new(match charge {
            Some(micros) => filler.charging(micros),
            None => filler,
        });
        self.0.made.lock().unwrap().push(filler.clone());
        Ok(Made::Fill(Arc::new(Handed(filler, self.0.clone()))))
    }
}

/// The kinds of this build with the fake kind registered beside them.
pub fn kinds_with(fakes: &Arc<Fakes>) -> Kinds {
    let mut kinds = wiring::kinds(&OneWorker::installed());
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

/// `paints`: an action that needs a filler and asks it once, or as many times as its
/// argument `times` says. It fails, as an error of the kind `fill`, at the first
/// request the filler answers with a fault. It counts its runs.
pub fn paints(runs: &Arc<AtomicU64>) -> Arc<dyn Action> {
    let times = ValueType::integer(Some(1), Some(64));
    let draft = ActionDescriptor::draft("paints", "1.0.0", Kind::Primitive, "A test action.")
        .argument(Argument::optional("times", "How many requests.", times))
        .needs(Requirement::new(Capability::Fill));
    Arc::new(Scripted {
        descriptor: draft.build().unwrap(),
        runs: runs.clone(),
        body: Box::new(|call, context| {
            let fill = context.capabilities().fill().expect("a filler is handed");
            let times = call
                .arguments()
                .iter()
                .find_map(|(name, value)| match value {
                    Value::Integer(times) if name == "times" => Some(*times),
                    _ => None,
                });
            for _ in 0..times.unwrap_or(1) {
                if let Err(fault) = fill.fill(&fill_request()) {
                    return Err(Failure::error("fill", &fault.to_string()));
                }
            }
            Ok(Produced::nothing())
        }),
    })
}

/// A scratch folder with a store in it, a place for a config file and an env file,
/// a models directory in which [`MODEL`] is installed, the fake kind, and a count of
/// the runs of `paints`.
pub struct World {
    dir: Scratch,
    pub root: PathBuf,
    pub fakes: Arc<Fakes>,
    pub runs: Arc<AtomicU64>,
}

/// The catalogue's entry for [`MODEL`] at this revision: one file, whose content
/// names the revision, so that two revisions are two installations.
fn entry(revision: &str) -> (Entry, FakeDownloader) {
    entry_named(MODEL, revision)
}

/// The same, for a model of this name.
fn entry_named(model: &str, revision: &str) -> (Entry, FakeDownloader) {
    let none = Secrets::none();
    let name = |text: &str| ModelName::new(text, &none).unwrap();
    let content = format!("weights of {revision}").into_bytes();
    let from = Address::of("https://weights.example.org/files/weights.bin", &none).unwrap();
    let file = CatalogueFile::new(
        FileName::new("weights.bin", &none).unwrap(),
        content.len() as u64,
        Digest::of_bytes(&content),
        from.clone(),
        &none,
    );
    let parts = EntryParts {
        name: name(model),
        title: Title::new("A model of the tests", &none).unwrap(),
        revision: Revision::new(revision, &none).unwrap(),
        adapter: name("fake-adapter"),
        licence: Licence::new("Apache-2.0", &none).unwrap(),
        platforms: Platform::ALL.to_vec(),
        features: Vec::new(),
        origins: vec![Origin::of("https://weights.example.org", &none).unwrap()],
        files: vec![file.unwrap()],
    };
    let entry = Entry::new(parts, &none).unwrap();
    (entry, FakeDownloader::new().serving(&from, content))
}

/// The revision the one adapter of the tests states of itself.
fn adapter() -> Revision {
    Revision::new("7", &Secrets::none()).unwrap()
}

impl World {
    pub fn new(label: &str) -> World {
        let dir = Scratch::new(label);
        let root = dir.path().join("store");
        drop(Store::open(&root, Arc::new(RealFs)).unwrap());
        std::fs::create_dir(dir.path().join("models")).unwrap();
        let world = World {
            dir,
            root,
            fakes: Arc::new(Fakes::default()),
            runs: Arc::new(AtomicU64::new(0)),
        };
        world.install("2026-11.1");
        world
    }

    /// The models directory of this world, in which [`MODEL`] is installed.
    pub fn models_dir(&self) -> PathBuf {
        self.dir.path().join("models")
    }

    /// A folder of this name in the world's scratch folder, which nothing has made.
    pub fn beside(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// The test's catalogue, which holds [`MODEL`] at its first revision, under the
    /// one adapter of the tests: what the binary's wiring is given in place of the
    /// catalogue it ships, with no word yet of where the directory is.
    pub fn models(&self) -> Models {
        let (first, _) = entry("2026-11.1");
        let adapters = vec![(first.adapter().clone(), adapter())];
        Models::of(
            Catalogue::new(vec![first], &Secrets::none()).unwrap(),
            adapters,
        )
    }

    /// What is installed in this world's models directory, for the catalogue's
    /// first revision, as a test looks at it: over the folder the test made.
    pub fn installed(&self) -> Installations {
        let (first, _) = entry("2026-11.1");
        Installations::of(
            ModelsDir::at(Arc::new(RealFs), &self.models_dir()),
            Catalogue::new(vec![first.clone()], &Secrets::none()).unwrap(),
            [(first.adapter().clone(), adapter())],
            Secrets::none(),
        )
    }

    /// Installs [`MODEL`] at this revision of its entry, with a fake of the download:
    /// the identity of the installation the model's pointer then names.
    pub fn install(&self, revision: &str) -> String {
        let (entry, downloads) = entry(revision);
        let dir = ModelsDir::at(Arc::new(RealFs), &self.dir.path().join("models"));
        let none = Secrets::none();
        let record = install(
            &dir,
            &entry,
            &adapter(),
            &downloads,
            &Cancellation::new(),
            &none,
        );
        record.unwrap().identity().digest().to_string()
    }

    pub fn config_path(&self) -> PathBuf {
        self.dir.path().join("whizbang.toml")
    }

    pub fn env_path(&self) -> PathBuf {
        self.dir.path().join("keys.env")
    }

    /// Writes the config file, and answers what a command that names it and this
    /// world's models directory (`--models-dir`) is built over: that file, no env
    /// file, the kinds with the fake, and the models of the test's catalogue, in the
    /// directory the binary's own wiring opened.
    pub fn config(&self, text: &str) -> Over {
        std::fs::write(self.config_path(), text).unwrap();
        self.named()
            .installed(self.models().flag(Some(&self.models_dir())))
            .expect("the models directory is opened")
    }

    /// What a command that names this world's config file is built over, before
    /// anything is said of models: the file, no env file, and the kinds with the
    /// fake.
    pub fn named(&self) -> Over {
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

    /// Every name the store holds under `spend/`: the ledger's books.
    pub fn books(&self) -> Vec<String> {
        let reader = Reader::open(&self.root, Arc::new(RealFs)).unwrap();
        let generation = reader.snapshot().unwrap();
        let names = generation.entries().keys();
        names
            .filter(|name| name.starts_with("spend/"))
            .cloned()
            .collect()
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
    let root = root.to_str().expect("the store's path is UTF-8");
    let mut all = args.to_vec();
    all.extend(["--store", root]);
    whizbang_storeless(&all)
}

/// The command that starts the real binary, with no argument yet, as the test kit makes
/// it: for a test that needs the command itself, such as one that reads the file it
/// names (`release`).
pub fn binary() -> whizbang_testkit::binary::Binary {
    whizbang_testkit::binary::command(env!("CARGO_BIN_EXE_whizbang"))
}

/// The real binary, with these arguments and no store: for a command that takes none.
pub fn whizbang_storeless(args: &[&str]) -> Ran {
    let out = binary().args(args).output().expect("the binary runs");
    Ran {
        code: out.status.code(),
        out: String::from_utf8(out.stdout).expect("output is UTF-8"),
        err: String::from_utf8(out.stderr).expect("output is UTF-8"),
    }
}

/// The version of the one model a run's record names, whole, as `runs show --json`
/// gives it.
pub fn version_of(root: &Path, run: &str) -> String {
    let json = whizbang(&["runs", "show", run, "--json"], root);
    assert_eq!(json.code, Some(0), "{}", json.err);
    let record: serde_json::Value = serde_json::from_str(&json.out).unwrap();
    let models = record["models"].as_array().unwrap();
    assert_eq!(models.len(), 1, "{models:?}");
    models[0]["version"].as_str().unwrap().to_owned()
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
    // The sentence is the one what a config offers says, of a config that offers
    // nothing: an executor that asked the backends it was built with, which are
    // none, would say "no backend is configured for fill".
    let why = not_ready(executor.submit(&Call::new("paints")));
    assert_eq!(
        why,
        "no default backend is configured for fill, and the call chose none"
    );
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
    // The build registers three kinds, `pack` (`wiring::kinds`, task P6a of plan 1C),
    // `service` (plan 2A, task S1) and `remote` (plan m6, task R1), so a backend of any
    // other kind a config names
    // is offered and is not ready, with the reason; nothing answers in its place.
    let world = World::new("wired-no-kind");
    let config = "[backends.fill]\nkind = \"fake\"\nmodel = \"paint-1\"\n";
    std::fs::write(world.config_path(), config).unwrap();
    let worker = OneWorker::installed();
    let kinds = wiring::kinds(&worker);
    assert_eq!(kinds.len(), 3);
    assert!(kinds.get("pack").is_some() && kinds.get("service").is_some());
    assert!(kinds.get("remote").is_some());
    assert!(kinds.get("fake").is_none());
    let over = Over::nothing()
        .config(Some(&world.config_path()))
        .kinds(kinds)
        .pack(worker);
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
    // The model is local: it was admitted under the identity of its installation,
    // which is the version its record names.
    has_line(&text, "models: 1");
    let identity = world.installed().ready(MODEL).unwrap().identity().digest();
    assert_eq!(
        version_of(&world.root, finished.run().as_str()),
        identity.to_string()
    );
    assert!(text.contains("  fill\tfake paint-1 sha256:"), "{text}");
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
    // The cap is held against what the call may cost, which is the bound of the
    // one request its need is for: a quarter of a dollar. Raised in the file to
    // that, it is the cap the next call is held to, and that call is admitted.
    drop(executor);
    reset(&world.root, "0.25");
    std::fs::write(world.config_path(), capped("0.25")).unwrap();
    let executor = world.runner(&over);
    executor.submit(&Call::new("paints")).unwrap();
    assert_eq!((world.ran(), world.fakes.requests()), (1, 1));
}

#[test]
fn a_request_that_is_both_too_old_and_over_the_cap_is_refused_as_stale() {
    // The parts are asked in the order they were given, and the first refusal is
    // the answer: the age's part stands before the cap's.
    let world = World::new("wired-order");
    let clock = Arc::new(Hand(AtomicU64::new(1_000_000)));
    let executor = world
        .runner(&world.config(&capped("0.10")))
        .with_clock(clock.clone());
    let prepared = executor.prepare(&Call::new("paints")).unwrap();
    assert!(prepared.estimate().micros() > 100_000);
    clock.0.fetch_add(PREPARATION_HOLDS_MS, Ordering::SeqCst);
    let request = Request::new(Call::new("paints")).expecting(&prepared);
    let refused = executor.request(&request);
    let Err(CallError::Refused(Refusal::ByPart { part, .. })) = refused else {
        panic!("not refused by a part: {refused:?}");
    };
    assert_eq!(part, AGE_PART);
    // Young again, the same request is refused by the cap.
    clock.0.fetch_sub(1, Ordering::SeqCst);
    let refused = executor.request(&request);
    let Err(CallError::Refused(Refusal::ByPart { part, .. })) = refused else {
        panic!("not refused by a part: {refused:?}");
    };
    assert_eq!(part, CAP_PART);
    assert!(world.held().is_empty());
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
    // The call may cost a quarter of a dollar: the bound of its one request.
    let world = World::new("wired-parts");
    let over = world.config(&capped("0.25"));
    reset(&world.root, "0.25");
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
    // the same, which is nothing. The config is used, and its model, which is the
    // key, is none the catalogue holds.
    drop(executor);
    let untold = world.runner(&over.clone().knowing(Secrets::none()));
    let prepared = untold.prepare(&Call::new("paints")).unwrap();
    assert!(matches!(prepared.readiness(), Readiness::NotReady(_)));
}

#[test]
fn the_value_of_a_variable_the_config_names_is_taken_out_of_what_a_run_under_the_binarys_executor_writes()
 {
    // The env file named beside the config holds the value of the variable the
    // config names: that value is the executor's secret from the first call on. A
    // value of the env file that the config does not name is no secret of the
    // executor's, and nothing carries one into a run.
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
    // Without the env file the config is used, and its backend that names a
    // variable is handed no key; the command asks for the env file before it builds
    // an executor that would use that backend (ADR 0011, "When the env file is asked
    // for"), and the name, which could be a key pasted in its place, is kept out.
    drop(executor);
    let over = world.config(text);
    let refused = wiring::env_file_asked(&over, Uses::One(Capability::Fill, None)).unwrap_err();
    assert_eq!(
        refused,
        "the fill backend `default` names its credential with `key_env`, and no env file was \
         given: name the env file that sets it with --env-file; nothing was read but the \
         config, and nothing ran"
    );
    assert_eq!(wiring::env_file_asked(&over, Uses::Nothing), Ok(()));
    let executor = world.runner(&over);
    let said = "the variable is FAKE_KEY";
    let finished = executor
        .submit(&Call::new("echo").argument("text", Value::text(said)))
        .unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    for bytes in world.bytes() {
        let found = bytes.windows(8).any(|at| at == b"FAKE_KEY");
        assert!(!found, "a file of the store holds the variable's name");
    }
}

// --- the policy --------------------------------------------------------------------------------

#[test]
fn a_recipient_the_config_does_not_approve_is_refused_at_the_gate_and_one_its_egress_table_approves_is_let_through()
 {
    // A backend with an address names that origin as its recipient. The policy is
    // the config's `egress` table and nothing else: with no table nothing is
    // approved, and the request is refused at the gate's place, before its backend
    // is called.
    // A backend that names a recipient is paid for, or it has no bound and its
    // call is not admitted: this one states a price, and the store an allowance.
    let world = World::new("wired-egress");
    let backend = "[backends.fill]\nkind = \"fake\"\nurl = \"https://api.example.com/v1\"\n\n\
                   [backends.fill.settings]\nat_most_micros = 250000\n";
    reset(&world.root, "20");
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
    // The gate stands before the meter: nothing was held for the request, and the
    // run cost nothing.
    has_line(
        &text,
        "spend: $0.25 reserved; paid requests: none; cost: $0.00",
    );
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

// --- the sender --------------------------------------------------------------------------------

#[test]
fn the_binarys_sender_refuses_a_file_from_inside_the_store_root() {
    // The binary's executor sends through the network client, with the check behind a
    // send over it keeping out the root of its store (plan 2A, task S1). A backend
    // approved for prompts sends, to a listener on loopback, words that name a file
    // inside the store: as its root is written, and as its canonical spelling. Each is
    // refused by the check, for the folder kept out, and the listener sees nothing.
    let world = World::new("wired-sender");
    let listener = whizbang_testkit::loopback::Listener::serve(1, |_, _| {});
    let origin = format!("https://127.0.0.1:{}", listener.port());
    let config = format!(
        "[backends.fill]\nkind = \"fake\"\nurl = \"{origin}/v1\"\n\n\
         [backends.fill.settings]\nat_most_micros = 250000\n\n\
         [egress.recipients.loopback]\norigin = \"{origin}\"\nclasses = [\"prompts\"]\n"
    );
    reset(&world.root, "20");
    let canonical = std::fs::canonicalize(&world.root).unwrap();
    for root in [&world.root, &canonical] {
        let named = format!("see {}/runs/a-file", root.display());
        world
            .fakes
            .posting(vec![Piece::of(DataClass::Prompts, named.into_bytes())]);
        let executor = world.runner(&world.config(&config));
        let refused = executor.submit(&Call::new("paints")).unwrap();
        assert_eq!(refused.outcome().name(), "error");
        drop(executor);
        let text = shown(&world.root, refused.run().as_str());
        assert!(text.contains("\trequest_not_approved\t"), "{text}");
        assert!(
            text.contains("the request holds the path of a folder that is kept out"),
            "{text}"
        );
    }
    assert_eq!(listener.accepted(), 0);
    // Words that name no folder are handed to the real sender, which connects to the
    // listener: it is the client, and not a sender that sends nothing. The listener
    // speaks no TLS, so nothing more happens.
    world
        .fakes
        .posting(vec![Piece::of(DataClass::Prompts, b"words".to_vec())]);
    let executor = world.runner(&world.config(&config));
    let sent = executor.submit(&Call::new("paints")).unwrap();
    assert_eq!(sent.outcome().name(), "error");
    drop(executor);
    assert_eq!(listener.finish(), 1);
}

#[test]
fn a_judge_call_to_a_recipient_the_config_does_not_name_is_refused_before_a_byte_leaves() {
    // A judge of the kind `service` whose config approves no recipient, and then one
    // that approves another origin: the gate refuses each request before anything is
    // handed to the sender, and the server sees no connection.
    use crate::anthropic::Judged;
    let judged = Judged::new("wired-unnamed");
    let listener = whizbang_testkit::loopback::Listener::serve(1, |_, _| {});
    let address = format!("https://127.0.0.1:{}/v1/messages", listener.port());
    for approved in [None, Some("https://api.example.com")] {
        let executor = judged.runner(&judged.over(&address, approved));
        let refused = executor.submit(&Call::new("judges")).unwrap();
        assert_eq!(refused.outcome().name(), "error");
        drop(executor);
        let text = shown(&judged.world.root, refused.run().as_str());
        assert!(text.contains("\trequest_not_approved\t"), "{text}");
    }
    assert_eq!(listener.finish(), 0);
    assert!(!judged.store_holds_the_key());
}

#[test]
fn a_google_or_openai_call_to_a_recipient_the_config_does_not_name_is_refused_before_a_byte_leaves()
{
    // As for Anthropic's judge, above, for each of the two others: a config that approves
    // no recipient, and then one that approves another origin; the gate refuses each
    // request before anything is handed to the sender, and the server sees no connection.
    use crate::anthropic::{GOOGLE, Judged, OPENAI};
    for provider in [GOOGLE, OPENAI] {
        let judged = Judged::of(&format!("wired-unnamed-{}", provider.name), provider);
        let listener = whizbang_testkit::loopback::Listener::serve(1, |_, _| {});
        let address = judged.address(listener.port());
        for approved in [None, Some("https://api.example.com")] {
            let executor = judged.runner(&judged.over(&address, approved));
            let refused = executor.submit(&Call::new("judges")).unwrap();
            assert_eq!(refused.outcome().name(), "error");
            drop(executor);
            let text = shown(&judged.world.root, refused.run().as_str());
            assert!(text.contains("\trequest_not_approved\t"), "{text}");
        }
        assert_eq!(listener.finish(), 0, "{}", provider.name);
        assert!(!judged.store_holds_the_key());
    }
}

#[test]
fn a_redirect_to_another_origin_is_refused_and_the_key_is_not_sent_there() {
    // The approved origin answers the judge's request by pointing it to another
    // origin, on another port of loopback: the backend asks the gate of that address,
    // which is not the recipient's, and is refused; the other server sees no
    // connection, so neither the key nor the request reached it.
    use crate::anthropic::{Judged, serving};
    let judged = Judged::new("wired-redirect");
    let elsewhere = whizbang_testkit::loopback::Listener::serve(1, |_, _| {});
    let there = format!("https://127.0.0.1:{}/v1/messages", elsewhere.port());
    let moved = format!("HTTP/1.1 307 Temporary Redirect\r\nLocation: {there}\r\n\r\n");
    let (listener, log) = serving(&judged.pki, vec![moved.into_bytes()]);
    let origin = format!("https://127.0.0.1:{}", listener.port());
    let executor = judged.runner(&judged.over(&format!("{origin}/v1/messages"), Some(&origin)));
    let refused = executor.submit(&Call::new("judges")).unwrap();
    assert_eq!(refused.outcome().name(), "error");
    drop(executor);
    assert_eq!(listener.finish(), 1);
    assert_eq!(elsewhere.finish(), 0);
    // The approved origin was sent the key; the other was sent nothing.
    assert!(log.lock().unwrap()[0].0.contains(&judged.key));
    let text = shown(&judged.world.root, refused.run().as_str());
    assert!(text.contains("\trequest_not_approved\t"), "{text}");
    assert!(
        text.contains("the origin is neither the recipient"),
        "{text}"
    );
    assert!(!judged.store_holds_the_key());
}

// --- the ledger, the bound of a call and the meter ---------------------------------------------

/// A config whose one filler states a bound of a quarter of a dollar for a request
/// and reports this charge with every answer, or none.
fn priced(charges_micros: Option<u64>) -> String {
    let charge = charges_micros.map_or(String::new(), |micros| {
        format!("charges_micros = {micros}\n")
    });
    format!(
        "[backends.fill]\nkind = \"fake\"\n\n[backends.fill.settings]\nat_most_micros = 250000\n{charge}"
    )
}

/// The owner's reset, by the real binary.
fn reset(root: &Path, allowance: &str) {
    let reset = whizbang(&["spend", "reset", "--allowance", allowance], root);
    assert_eq!(reset.code, Some(0), "{}", reset.err);
}

/// What `spend status` says, by the real binary.
fn status(root: &Path) -> String {
    let status = whizbang(&["spend", "status"], root);
    assert_eq!(status.code, Some(0), "{}", status.err);
    status.out
}

fn over_allowance(refused: Result<impl std::fmt::Debug, CallError>) -> (u64, u64) {
    match refused {
        Err(CallError::Refused(Refusal::OverAllowance {
            action,
            asked_micros,
            left_micros,
        })) => {
            assert_eq!(action, "paints");
            (asked_micros, left_micros)
        }
        other => panic!("not refused as over the allowance: {other:?}"),
    }
}

#[test]
fn a_paid_call_of_one_request_is_admitted_under_a_cap_and_an_allowance_that_cover_one_bound() {
    // `paints` says nothing of how many requests its need is for, so it is for one,
    // and a call of it may cost the bound of one: a quarter of a dollar. A cap of one
    // call and an allowance of that much each cover it.
    let world = World::new("wired-one-bound");
    let over = world.config(&capped("0.25"));
    reset(&world.root, "0.25");
    let executor = world.runner(&over);
    let prepared = executor.prepare(&Call::new("paints")).unwrap();
    assert_eq!(prepared.estimate().micros(), 250_000);
    let finished = executor.submit(&Call::new("paints")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    assert_eq!((world.ran(), world.fakes.requests()), (1, 1));
    // The read-only executor is told the same.
    drop(executor);
    let reader = Arc::new(Reader::open(&world.root, Arc::new(RealFs)).unwrap());
    let preparer = wiring::executor(world.registry(), reader, &over).unwrap();
    let prepared = preparer.prepare(&Call::new("paints")).unwrap();
    assert_eq!(prepared.estimate().micros(), 250_000);
    has_line(
        &shown(&world.root, finished.run().as_str()),
        "spend: $0.25 reserved; paid requests: 1; cost: $0.25",
    );
}

#[test]
fn of_the_actions_this_build_ships_check_alone_needs_a_backend_and_states_the_turns_of_its_judge() {
    // No number of requests is given to every action alike: each need states its
    // own (ADR 0010). `check` sends its judge a request for each turn of its loop,
    // and says so; `grow` and `view` need no backend.
    let registry = wiring::registry().unwrap();
    let needs = |name: &str| {
        let needs = registry.get(name).unwrap().descriptor().needs().to_vec();
        let stated = needs.iter().map(|need| (need.capability(), need.sends()));
        stated.collect::<Vec<_>>()
    };
    let turns = u64::try_from(whizbang_squint::check::TURNS_LIMIT).unwrap();
    assert_eq!(needs("check"), [(Capability::Judge, turns)]);
    assert_eq!(turns, 16);
    assert_eq!(needs("grow"), []);
    assert_eq!(needs("view"), []);
}

/// `twice`: an action whose need for a filler is for two requests, and that calls
/// `paints` once and sends nothing itself.
fn twice() -> Arc<dyn Action> {
    let draft = ActionDescriptor::draft("twice", "1.0.0", Kind::Recipe, "A test action.");
    let descriptor = draft.needs(Requirement::new(Capability::Fill).requests(2));
    Arc::new(Scripted {
        descriptor: descriptor.build().unwrap(),
        runs: Arc::new(AtomicU64::new(0)),
        body: Box::new(|_, context| {
            let called = context.executor().submit(&Call::new("paints"));
            called.map_err(|error| Failure::error("called", &error.to_string()))?;
            Ok(Produced::nothing())
        }),
    })
}

#[test]
fn a_run_called_from_inside_a_run_takes_the_sum_of_its_own_needs_from_what_its_caller_reserved() {
    // The caller's need is for two requests, so it reserves two bounds. The run it
    // calls needs one, and takes one from what its caller has: it is held to no
    // number but its own needs'.
    let world = World::new("wired-nested");
    let over = world.config(&priced(None));
    reset(&world.root, "0.5");
    let store = Arc::new(Store::open(&world.root, Arc::new(RealFs)).unwrap());
    let mut registry = world.registry();
    registry.register(twice()).unwrap();
    let executor = wiring::runner(registry, store, &over).unwrap();
    let finished = executor.submit(&Call::new("twice")).unwrap();
    assert_eq!(finished.outcome().name(), "completed", "{finished:?}");
    assert_eq!(world.fakes.requests(), 1);
    drop(executor);
    let reserved: Vec<(String, Option<u64>)> = world
        .held()
        .iter()
        .map(|(run, _)| {
            let json = whizbang(&["runs", "show", run, "--json"], &world.root);
            let record: serde_json::Value = serde_json::from_str(&json.out).unwrap();
            let action = record["asked"]["action"].as_str().unwrap().to_owned();
            (action, record["spend"]["reserved_micros"].as_u64())
        })
        .collect();
    let of = |action: &str| reserved.iter().find(|(name, _)| name == action).unwrap().1;
    assert_eq!(of("twice"), Some(500_000));
    assert_eq!(of("paints"), Some(250_000));
}

/// How a call from inside a run was refused as over its caller: the action, what it
/// asked and what its caller had left; or whatever else came back, as words.
type OverCaller = Result<(String, u64, u64), String>;

/// `asks_paints`: an action with no needs, so a run of it may cost nothing, that calls
/// `paints` once from inside its run and keeps how that call was answered.
fn asks_paints(answers: &Arc<Mutex<Vec<OverCaller>>>) -> Arc<dyn Action> {
    let draft = ActionDescriptor::draft("asks_paints", "1.0.0", Kind::Recipe, "A test action.");
    let answers = answers.clone();
    Arc::new(Scripted {
        descriptor: draft.build().unwrap(),
        runs: Arc::new(AtomicU64::new(0)),
        body: Box::new(move |_, context| {
            let answer = match context.executor().submit(&Call::new("paints")) {
                Err(CallError::Refused(Refusal::OverCaller {
                    action,
                    asked_micros,
                    left_micros,
                })) => Ok((action, asked_micros, left_micros)),
                other => Err(format!("{other:?}")),
            };
            answers.lock().unwrap().push(answer);
            Ok(Produced::nothing())
        }),
    })
}

#[test]
fn a_paid_call_from_inside_a_run_that_may_cost_nothing_is_refused_as_over_its_caller_with_nothing_sent_and_the_books_unchanged()
 {
    // The store allows five dollars, which would cover the paid call many times
    // over if it were made by a caller. It is made from inside a run that needs no
    // backend: that run was admitted without the ledger, with nothing to spend, and
    // a call from inside a run draws on its caller and never on the store's books.
    let world = World::new("wired-nested-free");
    let over = world.config(&priced(Some(100_000)));
    reset(&world.root, "5");
    let books = world.books();
    let answers: Arc<Mutex<Vec<OverCaller>>> = Arc::default();
    let store = Arc::new(Store::open(&world.root, Arc::new(RealFs)).unwrap());
    let mut registry = world.registry();
    registry.register(asks_paints(&answers)).unwrap();
    let executor = wiring::runner(registry, store, &over).unwrap();
    let finished = executor.submit(&Call::new("asks_paints")).unwrap();
    assert_eq!(finished.outcome().name(), "completed", "{finished:?}");
    drop(executor);
    // Refused as over its caller: a quarter of a dollar asked, nothing left.
    assert_eq!(
        *answers.lock().unwrap(),
        [Ok(("paints".to_owned(), 250_000, 0))]
    );
    // No run was made for it, its action did not run and no backend was sent
    // anything.
    assert_eq!((world.ran(), world.fakes.requests()), (0, 0));
    let held = world.held();
    assert_eq!(held.len(), 1, "{held:?}");
    // The books are as the reset left them: nothing was reserved, by the caller or
    // by the call, and the allowance is whole.
    assert_eq!(world.books(), books);
    let said = status(&world.root);
    assert!(said.contains("counted against it: $0.00\n"), "{said}");
    assert!(said.contains("left: $5.00\n"), "{said}");
    // The caller's record says that nothing was reserved for it, and holds the
    // refusal.
    let text = shown(&world.root, finished.run().as_str());
    has_line(
        &text,
        "spend: $0.00 reserved; paid requests: none; cost: $0.00",
    );
    assert!(text.contains("  paints\tover_caller\t"), "{text}");
}

#[test]
fn with_no_config_a_free_run_is_admitted_in_a_store_that_allows_nothing_and_the_books_name_nothing()
{
    // A store no reset was made in allows nothing. A call that needs no backend may
    // cost nothing: it is admitted without the ledger being asked, and its record
    // says that nothing was reserved, as an amount and not as null, because it ran
    // under a ledger with nothing to spend.
    let world = World::new("wired-free");
    let executor = world.runner(&Over::nothing());
    let finished = executor
        .submit(&Call::new("echo").argument("text", Value::text("hello")))
        .unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    drop(executor);
    let run = finished.run().as_str();
    has_line(
        &shown(&world.root, run),
        "spend: $0.00 reserved; paid requests: none; cost: $0.00",
    );
    let json = whizbang(&["runs", "show", run, "--json"], &world.root);
    let record: serde_json::Value = serde_json::from_str(&json.out).unwrap();
    assert_eq!(record["spend"]["reserved_micros"], 0);
    // No name under `spend/` was made for it: there is no reservation of nothing,
    // and nothing for `spend status` to count or to list.
    assert_eq!(world.books(), Vec::<String>::new());
    let said = status(&world.root);
    assert!(!said.contains(run), "{said}");
}

#[test]
fn a_paid_call_is_refused_without_an_allowance_and_admitted_with_one_and_runs_show_lists_its_request_with_what_it_was_charged()
 {
    let world = World::new("wired-paid");
    let over = world.config(&priced(Some(100_000)));
    // No reset was made in the store: it allows nothing, and the call may cost a
    // quarter of a dollar, the bound of its one request.
    let executor = world.runner(&over);
    assert_eq!(
        over_allowance(executor.submit(&Call::new("paints"))),
        (250_000, 0)
    );
    assert_eq!((world.ran(), world.fakes.requests()), (0, 0));
    assert!(world.held().is_empty());
    drop(executor);
    // The owner's reset sets an allowance, and the same call is admitted.
    reset(&world.root, "5");
    let executor = world.runner(&over);
    let finished = executor.submit(&Call::new("paints")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    assert_eq!((world.ran(), world.fakes.requests()), (1, 1));
    drop(executor);
    // `runs show` lists the paid request with what it was held to and what it was
    // charged, in dollars, and says what the run cost.
    let run = finished.run().as_str();
    let text = shown(&world.root, run);
    has_line(
        &text,
        "spend: $0.25 reserved; paid requests: 1; cost: $0.10",
    );
    has_line(
        &text,
        "  request 0\tanswered\theld $0.25\tsettled $0.10\tunresolved $0.00",
    );
    // The run's end closed its reservation: what its request was charged stays
    // counted, and the rest is free for the next call.
    let said = status(&world.root);
    assert!(said.contains("allowance: $5.00\n"), "{said}");
    assert!(said.contains("counted against it: $0.10\n"), "{said}");
    assert!(said.contains("left: $4.90\n"), "{said}");
    let of_run = whizbang(&["spend", "status", "--run", run], &world.root);
    assert_eq!(of_run.code, Some(0), "{}", of_run.err);
    assert!(of_run.out.contains("$0.10"), "{}", of_run.out);
}

#[test]
fn a_request_beyond_what_its_need_stated_is_refused_unsent_by_the_meter_as_over_what_its_run_has_left()
 {
    // The ledger and the meter are given together: what a call reserved is what its
    // requests draw on, one bound for each before it leaves. `paints` states one
    // request; a run of it that asks twice is refused the second, unsent, whatever
    // the allowance of the store has.
    let world = World::new("wired-metered");
    let over = world.config(&priced(None));
    reset(&world.root, "5");
    let executor = world.runner(&over);
    let call = Call::new("paints").argument("times", Value::Integer(2));
    let finished = executor.submit(&call).unwrap();
    assert_eq!(finished.outcome().name(), "error");
    assert_eq!(world.fakes.requests(), 1);
    drop(executor);
    let text = shown(&world.root, finished.run().as_str());
    // No answer stated a charge, so the request is settled at its bound.
    has_line(
        &text,
        "spend: $0.25 reserved; paid requests: 1; cost: $0.25",
    );
    has_line(
        &text,
        "  request 0\tanswered\theld $0.25\tsettled $0.25\tunresolved $0.00",
    );
    assert!(!text.contains("  request 1\t"), "{text}");
    assert!(text.contains("\trequest_over_allowance\t"), "{text}");
    let said = status(&world.root);
    assert!(said.contains("counted against it: $0.25\n"), "{said}");
    assert!(said.contains("left: $4.75\n"), "{said}");
}

/// `dies`: an action that needs a filler, asks it once, and then has the store's
/// file system refuse the next call that changes the disk: the first of the
/// publication that ends its run. The run stays as it was admitted, which is what a
/// process that was killed while its action ran leaves.
fn dies(fs: &Arc<ShimFs>) -> Arc<dyn Action> {
    let draft = ActionDescriptor::draft("dies", "1.0.0", Kind::Primitive, "A test action.");
    let descriptor = draft.needs(Requirement::new(Capability::Fill)).build();
    let fs = fs.clone();
    Arc::new(Scripted {
        descriptor: descriptor.unwrap(),
        runs: Arc::new(AtomicU64::new(0)),
        body: Box::new(move |_, context| {
            let fill = context.capabilities().fill().expect("a filler is handed");
            fill.fill(&fill_request()).expect("the request is answered");
            fs.refuse_before(fs.events().len());
            Ok(Produced::nothing())
        }),
    })
}

#[test]
fn the_reservation_of_a_run_a_killed_process_left_is_named_by_spend_status_and_cleared_by_spend_reset_and_recovery_makes_no_name_of_the_ledgers()
 {
    let world = World::new("wired-killed");
    let over = world.config(&priced(Some(100_000)));
    reset(&world.root, "0.25");
    // A process is killed while its action runs, after its one paid request was
    // answered and settled.
    {
        let fs = Arc::new(ShimFs::new());
        let store = Arc::new(Store::open(&world.root, fs.clone()).unwrap());
        let mut registry = world.registry();
        registry.register(dies(&fs)).unwrap();
        let executor = wiring::runner(registry, store, &over).unwrap();
        let left = executor.submit(&Call::new("dies"));
        assert!(matches!(left, Err(CallError::Store(_))), "{left:?}");
    }
    assert_eq!(world.fakes.requests(), 1);
    let held = world.held();
    assert_eq!(held.len(), 1);
    let run = held[0].0.clone();
    // A command that reads recovers nothing: the run is as it was left, and its
    // reservation is named as open, counted in full, by whichever reader, however
    // often.
    let open = format!(
        "  {run}  open  counts for $0.25  (reserved $0.25, charged $0.10, outstanding $0.00)"
    );
    let books = world.books();
    for _ in 0..2 {
        let listed = whizbang(&["runs", "list"], &world.root);
        assert!(
            listed.out.contains("\tadmitted, not ended\t"),
            "{}",
            listed.out
        );
        let said = status(&world.root);
        has_line(&said, &open);
        assert!(said.contains("counted against it: $0.25\n"), "{said}");
        assert!(said.contains("left: $0.00\n"), "{said}");
        assert_eq!(world.books(), books);
    }
    // So a paid call is refused: what is left does not cover it.
    let executor = world.runner(&over);
    assert_eq!(
        over_allowance(executor.submit(&Call::new("paints"))),
        (250_000, 0)
    );
    drop(executor);
    assert_eq!(world.books(), books);
    // The next command that opens the store to write recovers it first: the run is
    // ended, as interrupted. The recovery makes no name of the ledger's: the
    // reservation is not closed, and is still named as open and counted in full.
    let pruned = whizbang(&["store", "prune"], &world.root);
    assert_eq!(pruned.code, Some(0), "{}", pruned.err);
    let listed = whizbang(&["runs", "list"], &world.root);
    assert!(
        listed.out.contains("\terror, interrupted ("),
        "{}",
        listed.out
    );
    assert_eq!(world.books(), books);
    has_line(&status(&world.root), &open);
    has_line(
        &shown(&world.root, &run),
        "spend: $0.25 reserved; paid requests: unknown; cost: unknown",
    );
    // The owner's reset clears it: nothing made before the reset counts, and a
    // paid call is admitted.
    reset(&world.root, "5");
    let said = status(&world.root);
    assert!(!said.contains(&run), "{said}");
    assert!(said.contains("left: $5.00\n"), "{said}");
    let executor = world.runner(&over);
    let finished = executor.submit(&Call::new("paints")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    assert_eq!(world.ran(), 1);
    // What the run that was left cost is still seen, by its run.
    drop(executor);
    let of_run = whizbang(&["spend", "status", "--run", &run], &world.root);
    assert_eq!(of_run.code, Some(0), "{}", of_run.err);
    assert!(of_run.out.contains("$0.10"), "{}", of_run.out);
}

#[test]
fn a_store_left_by_a_process_that_died_between_a_dispatch_and_its_settlement_counts_the_request_at_its_bound_until_the_owners_reset()
 {
    let world = World::new("wired-died-unsettled");
    let over = world.config(&priced(Some(100_000)));
    reset(&world.root, "0.25");
    // A process sends its one paid request and is gone before the settlement is in
    // the store: the dispatch is durable, the backend was asked, and the
    // publication of the settlement fails. Its run is abandoned, as it was admitted.
    {
        let fs = Arc::new(ShimFs::new());
        let store = Arc::new(Store::open(&world.root, fs.clone()).unwrap());
        world.fakes.die_once_a_request_has_left(&fs);
        let executor = wiring::runner(world.registry(), store, &over).unwrap();
        let left = executor.submit(&Call::new("paints"));
        assert!(matches!(left, Err(CallError::Abandoned { .. })), "{left:?}");
    }
    assert_eq!(world.fakes.requests(), 1);
    let held = world.held();
    assert_eq!(held.len(), 1);
    let run = held[0].0.clone();
    let books = world.books();
    let named = |part: &str| books.iter().filter(|name| name.contains(part)).count();
    assert_eq!((named("/dispatch"), named("/settl")), (1, 0), "{books:?}");
    // The answer said ten cents. No settlement says so, so the request is counted
    // at its bound, a quarter of a dollar, and nothing is left.
    let open = format!(
        "  {run}  open  counts for $0.25  (reserved $0.25, charged $0.00, outstanding $0.25)"
    );
    let said = status(&world.root);
    has_line(&said, &open);
    assert!(said.contains("left: $0.00\n"), "{said}");
    // Nothing sends the request again: a paid call is refused, and the next command
    // that opens the store to write ends the run as interrupted and leaves the books
    // as they are.
    let executor = world.runner(&over);
    assert_eq!(
        over_allowance(executor.submit(&Call::new("paints"))),
        (250_000, 0)
    );
    drop(executor);
    let pruned = whizbang(&["store", "prune"], &world.root);
    assert_eq!(pruned.code, Some(0), "{}", pruned.err);
    let listed = whizbang(&["runs", "list"], &world.root);
    assert!(
        listed.out.contains("\terror, interrupted ("),
        "{}",
        listed.out
    );
    assert_eq!(world.books(), books);
    has_line(&status(&world.root), &open);
    assert_eq!((world.ran(), world.fakes.requests()), (1, 1));
    // The owner's reset clears it, and a paid call is admitted.
    reset(&world.root, "5");
    let said = status(&world.root);
    assert!(!said.contains(&run), "{said}");
    assert!(said.contains("left: $5.00\n"), "{said}");
    *world.fakes.dies.lock().unwrap() = None;
    let executor = world.runner(&over);
    let finished = executor.submit(&Call::new("paints")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    assert_eq!((world.ran(), world.fakes.requests()), (2, 2));
}

// --- a call that may cost nothing ------------------------------------------------------------

/// The real file system, keeping the name of every file it was asked for the content
/// of: a stored file is named by the digest of its bytes.
#[derive(Default)]
struct Reads(Mutex<Vec<String>>);

impl Reads {
    fn noted(&self, path: &Path) {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        self.0.lock().unwrap().push(name.into_owned());
    }

    /// How many of these stored files it was asked for since it was last asked this.
    fn of(&self, files: &[String]) -> usize {
        let read = std::mem::take(&mut *self.0.lock().unwrap());
        read.iter().filter(|name| files.contains(name)).count()
    }
}

impl Fs for Reads {
    fn create_dir(&self, path: &Path) -> io::Result<()> {
        RealFs.create_dir(path)
    }
    fn write_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        RealFs.write_new(path, bytes)
    }
    fn sync_dir(&self, path: &Path) -> io::Result<()> {
        RealFs.sync_dir(path)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        RealFs.rename(from, to)
    }
    fn remove_all(&self, path: &Path) -> io::Result<()> {
        RealFs.remove_all(path)
    }
    fn read_at_most(&self, path: &Path, limit: u64) -> io::Result<Vec<u8>> {
        self.noted(path);
        RealFs.read_at_most(path, limit)
    }
    fn list(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        RealFs.list(path)
    }
    fn exists(&self, path: &Path) -> io::Result<bool> {
        RealFs.exists(path)
    }
    fn hold_lock_file(&self, path: &Path, create: bool) -> io::Result<Option<Box<dyn LockFile>>> {
        RealFs.hold_lock_file(path, create)
    }
    fn share(&self, path: &Path) -> io::Result<Box<dyn Held>> {
        RealFs.share(path)
    }
    fn try_alone(&self, path: &Path) -> io::Result<Option<Box<dyn Held>>> {
        RealFs.try_alone(path)
    }
    fn begin_new(&self, path: &Path, most: u64) -> io::Result<Box<dyn NewFile>> {
        RealFs.begin_new(path, most)
    }
    fn open_file(&self, path: &Path) -> io::Result<Box<dyn OpenFile>> {
        self.noted(path);
        RealFs.open_file(path)
    }
    fn try_share(&self, path: &Path) -> io::Result<Option<Box<dyn Held>>> {
        RealFs.try_share(path)
    }
    fn is_link(&self, path: &Path) -> io::Result<bool> {
        RealFs.is_link(path)
    }
}

/// The names this store holds under `spend/`, and the stored file of each.
fn books_of(store: &Store) -> (Vec<String>, Vec<String>) {
    let generation = store.snapshot().unwrap();
    let books = generation.entries().iter();
    let books = books.filter(|(name, _)| name.starts_with("spend/"));
    books
        .map(|(name, entry)| {
            let file = entry.digest.to_string();
            let file = file.rsplit(':').next().unwrap_or_default().to_owned();
            (name.clone(), file)
        })
        .unzip()
}

fn free() -> Call {
    Call::new("echo").argument("text", Value::text("hello"))
}

#[test]
fn a_free_run_makes_no_name_under_spend_and_reads_none_of_the_books_and_a_paid_run_does_both() {
    // The books hold something: the owner's reset, and the names of a paid run.
    let world = World::new("wired-free-unasked");
    let over = world.config(&priced(None));
    reset(&world.root, "5");
    let executor = world.runner(&over);
    executor.submit(&Call::new("paints")).unwrap();
    drop(executor);
    // A call that needs no paid backend may cost nothing, and is admitted without
    // the ledger being asked: its admission and its end make no name under
    // `spend/` and read the content of none, however many the store holds.
    let fs = Arc::new(Reads::default());
    let store = Arc::new(Store::open(&world.root, fs.clone()).unwrap());
    let executor = wiring::runner(world.registry(), store.clone(), &over).unwrap();
    let (books, files) = books_of(&store);
    assert!(books.len() > 6, "{books:?}");
    fs.of(&files);
    let finished = executor.submit(&free()).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    assert_eq!(fs.of(&files), 0);
    assert_eq!(books_of(&store).0, books);
    // The same executor asks the ledger for a paid call, which reads the books and
    // adds its names to them: what the free call was spared.
    executor.submit(&Call::new("paints")).unwrap();
    assert!(fs.of(&files) > 0);
    assert!(books_of(&store).0.len() > books.len());
    drop((executor, store));
    // Its record says that nothing was reserved, as an amount: it ran under a ledger,
    // with nothing to spend. And the books say nothing of it.
    let run = finished.run().as_str();
    has_line(
        &shown(&world.root, run),
        "spend: $0.00 reserved; paid requests: none; cost: $0.00",
    );
    let json = whizbang(&["runs", "show", run, "--json"], &world.root);
    let record: serde_json::Value = serde_json::from_str(&json.out).unwrap();
    assert_eq!(record["spend"]["reserved_micros"], 0);
    let said = status(&world.root);
    assert!(!said.contains(run), "{said}");
    assert!(said.contains("counted against it: $0.50\n"), "{said}");
}

fn needs_owner(refused: Result<impl std::fmt::Debug, CallError>) -> u64 {
    match refused {
        Err(CallError::Refused(Refusal::NeedsOwner { epoch, .. })) => epoch,
        other => panic!("not refused as needing the owner: {other:?}"),
    }
}

#[test]
fn a_free_run_is_admitted_with_no_allowance_with_nothing_left_and_when_the_books_need_their_owner()
{
    let world = World::new("wired-free-admitted");
    let over = world.config(&priced(None));
    let completes = |executor: &Engine| {
        let finished = executor.submit(&free()).unwrap();
        assert_eq!(finished.outcome().name(), "completed");
    };
    // No allowance: no reset was made in the store. A paid call is refused.
    let executor = world.runner(&over);
    completes(&executor);
    let paid = executor.submit(&Call::new("paints"));
    assert_eq!(over_allowance(paid), (250_000, 0));
    drop(executor);
    assert_eq!(world.books(), Vec::<String>::new());
    // Nothing left: one paid call takes all of an allowance of a quarter.
    reset(&world.root, "0.25");
    let executor = world.runner(&over);
    executor.submit(&Call::new("paints")).unwrap();
    let paid = executor.submit(&Call::new("paints"));
    assert_eq!(over_allowance(paid), (250_000, 0));
    completes(&executor);
    drop(executor);
    // The books need their owner: other hands made a turn that no publication
    // took. Every paid call is refused until the owner resets, and a free one runs.
    let store = Arc::new(Store::open(&world.root, Arc::new(RealFs)).unwrap());
    let turn = Record::Turn {
        epoch: 1,
        turn: std::num::NonZeroU64::new(9).unwrap(),
        by: TurnBy::Settlement,
        run: None,
    };
    let mut publication = store.begin().unwrap();
    let kind = whizbang_core::store::Kind::Diagnostic;
    publication
        .create(&turn.name().to_string(), kind, &turn.to_bytes())
        .unwrap();
    publication.publish().unwrap();
    let executor = wiring::runner(world.registry(), store, &over).unwrap();
    assert_eq!(needs_owner(executor.submit(&Call::new("paints"))), 1);
    completes(&executor);
    drop(executor);
    let books = world.books();
    // No free run is named by the books: they hold the reset, the one paid run and
    // the planted turn.
    let runs: Vec<String> = world.held().into_iter().map(|(run, _)| run).collect();
    let named = runs
        .iter()
        .filter(|run| books.iter().any(|name| name.contains(run.as_str())));
    assert_eq!((runs.len(), named.count()), (4, 1), "{books:?}");
}

// --- the inventory and the lease ---------------------------------------------------------------

#[test]
fn a_local_backend_is_ready_only_where_the_models_directory_holds_its_model_installed_and_is_never_ready_under_no_inventory()
 {
    let world = World::new("wired-inventory");
    let local = |model: &str| format!("[backends.fill]\nkind = \"fake\"\nmodel = \"{model}\"\n");
    // A command that names no models directory has the inventory that knows no
    // model: a local backend is not ready, and says why. No other answers for it.
    std::fs::write(world.config_path(), local(MODEL)).unwrap();
    let over = Over::nothing()
        .config(Some(&world.config_path()))
        .kinds(kinds_with(&world.fakes));
    let executor = world.runner(&over);
    assert_eq!(
        not_ready(executor.submit(&Call::new("paints"))),
        "the fill backend default is local, and its model is not ready: it is not in the \
         catalogue"
    );
    drop(executor);
    // Over the models directory, the model the catalogue holds and the directory
    // has installed is ready, and its run is admitted and answered.
    let executor = world.runner(&world.config(&local(MODEL)));
    let finished = executor.submit(&Call::new("paints")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    assert_eq!((world.ran(), world.fakes.requests()), (1, 1));
    // A model the catalogue does not hold is not ready.
    std::fs::write(world.config_path(), local("paint-2")).unwrap();
    assert_eq!(
        not_ready(executor.submit(&Call::new("paints"))),
        "the fill backend default is local, and its model is not ready: it is not in the \
         catalogue"
    );
    // Installed again from another revision of its entry than the catalogue's, the
    // model is out of date: the inventory answers from what is on the disk now.
    std::fs::write(world.config_path(), local(MODEL)).unwrap();
    world.install("2026-11.2");
    assert_eq!(
        not_ready(executor.submit(&Call::new("paints"))),
        "the fill backend default is local, and its model is not ready: its installation \
         is out of date"
    );
    assert_eq!((world.ran(), world.fakes.requests()), (1, 1));
    assert_eq!(world.held().len(), 1);
}

/// `swaps`: an action that needs a filler, installs the model again from another
/// revision of its entry, and then asks the filler once. What the request came to
/// is what it returns: an error of the kind `fill` with the fault's sentence.
fn swaps(again: impl Fn() + Send + Sync + 'static) -> Arc<dyn Action> {
    let draft = ActionDescriptor::draft("swaps", "1.0.0", Kind::Primitive, "A test action.");
    let descriptor = draft.needs(Requirement::new(Capability::Fill)).build();
    Arc::new(Scripted {
        descriptor: descriptor.unwrap(),
        runs: Arc::new(AtomicU64::new(0)),
        body: Box::new(move |_, context| {
            let fill = context.capabilities().fill().expect("a filler is handed");
            again();
            match fill.fill(&fill_request()) {
                Ok(_) => Ok(Produced::nothing()),
                Err(fault) => Err(Failure::error("fill", fault.sentence())),
            }
        }),
    })
}

#[test]
fn a_request_to_a_local_model_takes_a_lease_on_the_installation_its_run_was_admitted_with_and_is_refused_unsent_once_that_is_replaced()
 {
    let world = Arc::new(World::new("wired-lease"));
    let over = world.config("[backends.fill]\nkind = \"fake\"\n");
    let admitted_with = world.installed().ready(MODEL).unwrap().identity().digest();
    let mut registry = world.registry();
    let again = world.clone();
    let replaced = Arc::new(Mutex::new(None));
    let noted = replaced.clone();
    registry
        .register(swaps(move || {
            *noted.lock().unwrap() = Some(again.install("2026-11.2"));
        }))
        .unwrap();
    let store = Arc::new(Store::open(&world.root, Arc::new(RealFs)).unwrap());
    let executor = wiring::runner(registry, store, &over).unwrap();
    // The run is admitted under the installation that was current; between its
    // admission and its request the model is installed again, as another
    // installation. The lease is for the one the admission recorded, which the
    // pointer no longer names: the request is refused before it leaves, and no
    // other installation answers in its place.
    let finished = executor.submit(&Call::new("swaps")).unwrap();
    let replaced = replaced.lock().unwrap().clone().unwrap();
    assert_ne!(replaced, admitted_with.to_string());
    assert_eq!(finished.outcome().name(), "error");
    assert_eq!(world.fakes.requests(), 0);
    drop(executor);
    let text = shown(&world.root, finished.run().as_str());
    // As it is printed, with its apostrophe escaped.
    let why = "the model\\'s installation was replaced or removed after the run was admitted";
    has_line(&text, &format!("error: fill: {why}"));
    has_line(&text, &format!("  none\trequest_not_ready\t{why}"));
    // The record names the model under the installation the run was admitted with,
    // and nothing was held or paid for the request.
    assert_eq!(
        version_of(&world.root, finished.run().as_str()),
        admitted_with.to_string()
    );
    has_line(
        &text,
        "spend: $0.00 reserved; paid requests: none; cost: $0.00",
    );
}

// --- the binary's inventory: where the directory is, the fifth reason, the secrets ------------------

/// A config whose one backend is local and asks for this model.
fn local(model: &str) -> String {
    format!("[backends.fill]\nkind = \"fake\"\nmodel = \"{model}\"\n")
}

/// What the inventory the binary's wiring made answers of [`MODEL`].
fn looked(over: &Over) -> Installed {
    over.installations().expect("an inventory").look(MODEL)
}

/// The permission bits of a folder.
#[cfg(unix)]
fn mode_of(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

#[test]
fn a_call_for_a_model_whose_adapter_this_build_lacks_is_refused_when_prepared_with_that_reason() {
    let world = World::new("wired-no-adapter");
    std::fs::write(world.config_path(), local(MODEL)).unwrap();
    let said = "the fill backend default is local, and its model is not ready: this build has \
                no adapter for it";
    let none = Secrets::none();
    let other = ModelName::new("another-adapter", &none).unwrap();
    // The model is in the catalogue and installed, by a build that had its adapter.
    // This build has another adapter: one of a test's, or the binary's own, which
    // since task R2a is `sam3`, and since task T1 of plan 1D `owlv2-sam2` beside it.
    let shipped: Vec<(ModelName, Revision)> = wiring::adapters()
        .into_iter()
        .filter(|adapter| adapter.shipped)
        .map(|adapter| (adapter.name, adapter.revision))
        .collect();
    let names: Vec<&str> = shipped.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["sam3", "owlv2-sam2"]);
    assert_ne!(MODEL, "sam3");
    // And the model's own adapter at its revision, named in the table as not yet
    // shipped (task N5): the call is refused when it is prepared, before any worker
    // could be started.
    let fake_adapter = ModelName::new("fake-adapter", &none).unwrap();
    for (adapters, not_yet) in [
        (vec![(other, adapter())], Vec::new()),
        (shipped, Vec::new()),
        (Vec::new(), vec![(fake_adapter, adapter())]),
    ] {
        let (first, _) = entry("2026-11.1");
        let catalogue = Catalogue::new(vec![first], &none).unwrap();
        let models = Models::of(catalogue, adapters)
            .not_yet_shipped(not_yet)
            .flag(Some(&world.models_dir()));
        let executor = world.runner(&world.named().installed(models).unwrap());
        // When the call is prepared: not ready, with the reason, and no model.
        let prepared = executor.prepare(&Call::new("paints")).unwrap();
        let Readiness::NotReady(missing) = prepared.readiness() else {
            panic!("the call is ready: {prepared:?}");
        };
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].why(), said);
        assert!(prepared.needs()[0].model().is_none());
        // And refused at admission the same way, with no run and nothing sent.
        assert_eq!(not_ready(executor.submit(&Call::new("paints"))), said);
    }
    assert_eq!((world.ran(), world.fakes.requests()), (0, 0));
    assert!(world.held().is_empty());
    // Under the adapter the model was installed with, the same call is ready.
    let executor = world.runner(&world.config(&local(MODEL)));
    let prepared = executor.prepare(&Call::new("paints")).unwrap();
    assert_eq!(prepared.readiness(), &Readiness::Ready);
}

#[cfg(unix)]
#[test]
fn the_models_directory_is_the_flags_then_the_configs_then_the_platforms() {
    use std::ffi::OsStr;
    // The choice, by itself.
    let (flag, config, platform) = (Path::new("/f"), Path::new("/c"), Path::new("/p"));
    for (given, chosen) in [
        (
            (Some(flag), Some(config), Some(platform)),
            Some((Said::Flag, flag)),
        ),
        ((Some(flag), None, None), Some((Said::Flag, flag))),
        (
            (None, Some(config), Some(platform)),
            Some((Said::Config, config)),
        ),
        (
            (None, None, Some(platform)),
            Some((Said::Platform, platform)),
        ),
        ((None, None, None), None),
    ] {
        assert_eq!(wiring::models_dir(given.0, given.1, given.2), chosen);
    }
    // The platform's, from what an environment says: nothing is read here.
    let home = Some(OsStr::new("/made-up/house"));
    let places = |macos, home, data: Option<&str>| {
        wiring::platform_models_dir(macos, home, data.map(OsStr::new))
            .map(|place| place.to_str().unwrap().to_owned())
    };
    assert_eq!(
        places(true, home, Some("/data")).as_deref(),
        Some("/made-up/house/Library/Application Support/whizbang/models")
    );
    assert_eq!(
        places(false, home, Some("/data")).as_deref(),
        Some("/data/whizbang/models")
    );
    for data in [None, Some(""), Some("data"), Some("./data")] {
        assert_eq!(
            places(false, home, data).as_deref(),
            Some("/made-up/house/.local/share/whizbang/models"),
            "{data:?}"
        );
    }
    for home in [None, Some(OsStr::new("")), Some(OsStr::new("someone"))] {
        assert_eq!(places(true, home, Some("/data")), None);
        assert_eq!(places(false, home, None), None);
    }

    // Through the binary's wiring. The model is installed in one models directory,
    // `kept`; each of the three names it or names another, which is empty. The
    // directory that was opened is the one whose answer the inventory gives.
    let world = World::new("wired-where");
    let kept = world.models_dir();
    let (other, third) = (world.beside("other"), world.beside("third"));
    let ready = |found: Installed| matches!(found, Installed::Ready(_));
    let absent = Installed::NotReady(Unready::NotInstalled);
    // Each config has a local backend: a config with none opens no directory (task
    // P6a), whichever of the three names one.
    let written = |dir: Option<&Path>| {
        let dir = dir.map(|dir| format!("[models]\ndir = '{}'\n", dir.to_str().unwrap()));
        let text = format!("{}{}", local(MODEL), dir.unwrap_or_default());
        std::fs::write(world.config_path(), text).unwrap();
    };
    // What says where the platform's is counts each time it is asked: it is asked
    // only when a directory is to be opened and neither of the others named one.
    let asked = Arc::new(AtomicU64::new(0));
    let says = |platform: Option<&Path>| -> wiring::Platform {
        let (asked, platform) = (asked.clone(), platform.map(Path::to_owned));
        Arc::new(move || {
            asked.fetch_add(1, Ordering::SeqCst);
            platform.clone()
        })
    };
    let over = |flag: Option<&Path>, platform: Option<&Path>| {
        world
            .named()
            .installed(world.models().flag(flag).platform(says(platform)))
    };
    // The flag, over the config's and the platform's.
    written(Some(&other));
    assert!(ready(looked(&over(Some(&kept), Some(&third)).unwrap())));
    written(Some(&kept));
    assert_eq!(looked(&over(Some(&other), Some(&kept)).unwrap()), absent);
    // With no flag, the config's, over the platform's.
    assert!(ready(looked(&over(None, Some(&third)).unwrap())));
    written(Some(&other));
    assert_eq!(looked(&over(None, Some(&kept)).unwrap()), absent);
    assert_eq!(
        asked.load(Ordering::SeqCst),
        0,
        "nobody asked for the platform's"
    );
    assert!(!third.exists());
    // With neither, the platform's, asked for once each time.
    written(None);
    assert!(ready(looked(&over(None, Some(&kept)).unwrap())));
    assert_eq!(looked(&over(None, Some(&third)).unwrap()), absent);
    assert_eq!(asked.load(Ordering::SeqCst), 2);
    // Each that was opened was made, its own folder alone, for its owner only.
    for made in [&other, &third] {
        assert_eq!(mode_of(made), 0o700, "{}", made.display());
    }
    // With none of the three there is no models directory, and the command says how
    // to name one.
    let refused = over(None, None).unwrap_err();
    assert!(
        refused.contains("--models-dir") && refused.contains("`dir`"),
        "{refused}"
    );

    // The folders above the platform's are made, for their owner only: nobody made
    // whizbang's data folder before its first use.
    let home = world.beside("home");
    let data = home.join("Library").join("Application Support");
    let place = data.join("whizbang").join("models");
    assert_eq!(looked(&over(None, Some(&place)).unwrap()), absent);
    for made in [&home, &data, &data.join("whizbang"), &place] {
        assert_eq!(mode_of(made), 0o700, "{}", made.display());
    }
    // The folders above one a person named are not: a path written wrong is said.
    let wrong = world.beside("no-such").join("models");
    let refused = over(Some(&wrong), Some(&place)).unwrap_err();
    assert!(
        refused.contains("could not be made or looked at"),
        "{refused}"
    );
    assert!(refused.contains("the one --models-dir names"), "{refused}");
    written(Some(&wrong));
    let refused = over(None, Some(&place)).unwrap_err();
    assert!(
        refused.contains("the config's `models.dir` names"),
        "{refused}"
    );
    assert!(!world.beside("no-such").exists());
    // No refusal holds a path.
    let scratch = world.root.parent().unwrap().to_str().unwrap();
    assert!(!refused.contains(scratch), "{refused}");

    // The directory is opened by the opening that asks whose it is: one that others
    // may write is refused, with how to mend it, whichever of the three named it.
    written(None);
    let open = world.beside("open");
    std::fs::create_dir(&open).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o777)).unwrap();
    }
    let refused = over(None, Some(&open)).unwrap_err();
    assert!(refused.contains("chmod go-w"), "{refused}");
    assert!(
        refused.contains("in this machine's data folder"),
        "{refused}"
    );
    assert!(std::fs::read_dir(&open).unwrap().next().is_none());

    // Where no model could be asked for, no directory is opened or made: under a
    // catalogue that holds none; under a config that has no local backend, with the
    // kinds this build has and the catalogue it ships (tasks N5 and P6a); and for a
    // command that names neither a config nor a directory. Where the platform's is, is
    // not even asked.
    // The kinds given after the directory: it is opened all the same, since which
    // backends are local is read from the config and not from the kinds (task P6a).
    written(None);
    let later = Over::nothing()
        .config(Some(&world.config_path()))
        .installed(world.models().flag(Some(&kept)))
        .unwrap()
        .kinds(kinds_with(&world.fakes));
    assert!(ready(looked(&later)));

    let never = world.beside("never");
    let before = asked.load(Ordering::SeqCst);
    let empty = Models::of(
        Catalogue::new(Vec::new(), &Secrets::none()).unwrap(),
        Vec::new(),
    );
    for flag in [None, Some(never.as_path())] {
        let models = empty.clone().flag(flag).platform(says(Some(&never)));
        let over = world.named().installed(models).unwrap();
        assert!(over.installations().is_none());
        // No backend, or remote ones alone, each naming a url: under the kinds this
        // build has, and under the fake's too, given before or after.
        let remote = "[backends.fill]\nkind = \"fake\"\nurl = \"https://fill.example\"\n";
        for config in ["", remote] {
            std::fs::write(world.config_path(), config).unwrap();
            let models = || {
                Models::shipped()
                    .unwrap()
                    .flag(flag)
                    .platform(says(Some(&never)))
            };
            let worker = OneWorker::installed();
            let over = world
                .named()
                .kinds(wiring::kinds(&worker))
                .pack(worker)
                .installed(models())
                .unwrap();
            assert!(over.installations().is_none());
            let over = Over::nothing()
                .config(Some(&world.config_path()))
                .installed(world.models().flag(flag).platform(says(Some(&never))))
                .unwrap()
                .kinds(kinds_with(&world.fakes));
            assert!(over.installations().is_none());
        }
    }
    let over = Over::nothing()
        .kinds(kinds_with(&world.fakes))
        .installed(world.models().platform(says(Some(&never))))
        .unwrap();
    assert!(over.installations().is_none());
    assert!(!never.exists());
    assert_eq!(asked.load(Ordering::SeqCst), before);
}

#[test]
fn under_the_binarys_wiring_a_call_for_an_installed_model_is_prepared_under_its_identity_and_holds_a_lease()
 {
    let world = World::new("wired-leased");
    let record = world.installed().ready(MODEL).unwrap();
    let identity = record.identity().digest().to_string();
    // The binary's wiring opened the directory and made the inventory; the test
    // gave it a catalogue and the place.
    let over = world.config(&local(MODEL));
    assert!(matches!(looked(&over), Installed::Ready(_)));
    let executor = world.runner(&over);
    let prepared = executor.prepare(&Call::new("paints")).unwrap();
    assert_eq!(prepared.readiness(), &Readiness::Ready);
    let model = prepared.needs()[0].model().expect("a model");
    assert_eq!((model.model(), model.version()), (MODEL, identity.as_str()));
    // While its request is out, the installation is held: the file a hold locks
    // cannot be taken alone, which is what a removal waits for. Before the request
    // and after it, nothing holds it.
    let hold_file = world
        .models_dir()
        .join("models")
        .join(MODEL)
        .join("holds")
        .join(record.identity().hex());
    let held = |file: &Path| RealFs.try_alone(file).unwrap().is_none();
    assert!(!held(&hold_file));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let (noted, file) = (seen.clone(), hold_file.clone());
    world
        .fakes
        .while_a_request_is_out(move || noted.lock().unwrap().push(held(&file)));
    let finished = executor.submit(&Call::new("paints")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    assert_eq!(*seen.lock().unwrap(), [true]);
    assert!(!held(&hold_file));
    drop(executor);
    assert_eq!(version_of(&world.root, finished.run().as_str()), identity);
}

#[test]
fn a_secret_the_executor_is_told_of_is_refused_as_a_models_name_by_the_binarys_inventory() {
    let world = World::new("wired-inventory-secret");
    // A config with a local backend, which is what opens a models directory (task
    // P6a); of another model than the one looked at here.
    std::fs::write(world.config_path(), local(MODEL)).unwrap();
    // A model whose name is a key, in a catalogue made by a reader that was told of
    // no secret, and installed.
    let key = made_up_key("model");
    let secrets = Secrets::new([key.clone()]).unwrap();
    let none = Secrets::none();
    let (named, downloads) = entry_named(&key, "2026-11.1");
    let dir = ModelsDir::at(Arc::new(RealFs), &world.models_dir());
    install(
        &dir,
        &named,
        &adapter(),
        &downloads,
        &Cancellation::new(),
        &none,
    )
    .unwrap();
    let models = || {
        let adapters = vec![(named.adapter().clone(), adapter())];
        Models::of(
            Catalogue::new(vec![named.clone()], &none).unwrap(),
            adapters,
        )
        .flag(Some(&world.models_dir()))
    };
    let look = |over: &Over| over.installations().unwrap().look(&key);
    let hidden = Installed::NotReady(Unready::NotInCatalogue);
    // Told of nothing, the binary's inventory finds the model ready.
    let untold = world.named().installed(models()).unwrap();
    assert!(matches!(look(&untold), Installed::Ready(_)));
    // Told of the key, it does not look for it: the name is a secret, and nothing
    // of that name is read from the disk. The inventory is told what the executor
    // is told, whichever is given first.
    let told_after = untold.clone().knowing(secrets.clone());
    assert_eq!(look(&told_after), hidden);
    let told_before = world.named().knowing(secrets.clone()).installed(models());
    let told_before = told_before.unwrap();
    assert_eq!(look(&told_before), hidden);
    // And told of nothing again, it is found again: one set, the last one given.
    assert!(matches!(
        look(&told_after.clone().knowing(none.clone())),
        Installed::Ready(_)
    ));
    // The executor built over the same is told the same: it takes the key out of
    // what a run writes.
    let executor = world.runner(&told_before);
    let said = format!("the key is {key}");
    executor
        .submit(&Call::new("echo").argument("text", Value::text(&said)))
        .unwrap();
    drop(executor);
    for bytes in world.bytes() {
        let found = bytes.windows(key.len()).any(|at| at == key.as_bytes());
        assert!(!found, "a file of the store holds the key");
    }
}

#[test]
fn two_configs_that_differ_only_in_where_the_models_directory_is_repeat_the_same_run() {
    // Where the models directory is, is not part of what a run means (task N3b): a
    // model is the identity of its installation, wherever its folder is. So a
    // config that moves the directory, and changes nothing else, is the config it
    // was: its digest is the same, a preparation made before the move is not
    // stale after it, and a request sent again under its id repeats its run.
    let world = World::new("wired-moved");
    let moved = world.beside("moved");
    let written = |dir: &Path, least: u64| {
        let text = format!(
            "{}\n[models]\ndir = '{}'\ndownload_least_kib_per_second = {least}\n",
            local(MODEL),
            dir.to_str().unwrap()
        );
        std::fs::write(world.config_path(), &text).unwrap();
        Config::parse(text.as_bytes(), &Secrets::none())
            .unwrap()
            .digest()
    };
    let before = written(&world.models_dir(), 9);
    let executor = world.runner(&world.named().installed(world.models()).unwrap());
    let call = Call::new("echo").argument("text", Value::text("hello"));
    let prepared = executor.prepare(&call).unwrap();
    assert_eq!(prepared.config(), before);
    // The config now names another directory, and another least rate.
    assert_eq!(written(&moved, 300), before);
    let id = executor.request_id("moved-1").unwrap();
    let request = Request::new(call).with_id(id).expecting(&prepared);
    let ran = executor.request(&request).unwrap();
    assert!(matches!(ran, Answer::Ran(_)), "{ran:?}");
    drop(executor);
    // A new command over the config as it is now opens the directory it names,
    // and the same request is answered with the run it made.
    let executor = world.runner(&world.named().installed(world.models()).unwrap());
    assert!(moved.is_dir());
    let again = executor.request(&request).unwrap();
    assert!(matches!(again, Answer::Repeat(_)), "{again:?}");
    assert_eq!(again.run(), ran.run());
    drop(executor);
    assert_eq!(world.held().len(), 1);
    has_line(
        &shown(&world.root, ran.run().as_str()),
        &format!("config: {before}"),
    );
}

#[test]
fn only_main_reads_where_the_platforms_data_folder_is() {
    // What says where the platform's data folder is, is something the wiring is
    // given. One function of the binary reads it from the environment, `main` alone
    // names it, handing it on uncalled, and nothing else in the binary reads the
    // environment at all: so no test of the wiring can be pointed at the data
    // folder of whoever runs it. The wiring calls what it was handed only when a
    // models directory is to be opened and nothing else named one
    // (`wired::the_models_directory_is_the_flags_then_the_configs_then_the_platforms`).
    let sources = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let (mut reads, mut calls, mut files) = (Vec::new(), Vec::new(), 0);
    // The texts searched for are kept in a data file, one a line, and not written
    // here: in a test of a crate that starts the binary, `just check` refuses them as
    // reads of the environment (scripts/check-binary-start.sh), and they are not.
    // An empty list would find nothing and pass: the five are required.
    let searched: Vec<&str> = include_str!("environment-reads.txt").lines().collect();
    assert_eq!(searched.len(), 5, "{searched:?}");
    assert!(searched.iter().all(|read| !read.is_empty()), "{searched:?}");
    for entry in std::fs::read_dir(&sources).unwrap() {
        let path = entry.unwrap().path();
        assert!(path.is_file(), "a folder of sources that is not looked at");
        let text = std::fs::read_to_string(&path).unwrap();
        let code: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        let name = path.file_name().unwrap().to_str().unwrap().to_owned();
        for read in &searched {
            reads.extend(code.matches(*read).map(|_| name.clone()));
        }
        calls.extend(code.matches("platforms_models_dir").map(|_| name.clone()));
        // Its name with nothing in the brackets is its definition, and never a call.
        let defined = code.matches("fnplatforms_models_dir()").count();
        let written = code.matches("platforms_models_dir()").count();
        assert_eq!(written, defined, "{name} calls it");
        files += 1;
    }
    assert!(files > 10, "{files} files of the binary were read");
    // `env::var_os` six times, each counted as `env::var` and as `var_os`: the home
    // folder and the data folder, in the one function; and the four variables that
    // say where the Hub's sign-in keeps its token, in the other (task N4,
    // `models::the_real_binary_started_with_a_temporary_home_reads_the_sign_in_there_and_no_other`).
    // And `setup`'s two (task Y1): `PATH` by `env::var_os`, counted twice, and the
    // whole environment by `env::vars_os`, counted as `env::var` and as `env::vars`,
    // from which `uv`'s own is chosen.
    assert_eq!(reads, ["main.rs"; 16], "the environment is read elsewhere");
    // The function's definition, and the seven places it is handed on: to the two
    // commands that build an executor (`rascal`, and `squint`, plan 2A's task S2), to
    // the models commands (task N3b), to `setup` (task Y1) and to `backends status`
    // (task P7b), which opens the models directory a local backend's model is looked
    // up in; and to `serve` (plan 2C, task M1), which builds an executor as `squint`
    // does; and to `lab run` (plan m3, task L2), which builds one to run a round; and
    // to `play` (plan m5, task H1), which builds one as `serve` does.
    assert_eq!(calls, ["main.rs"; 9], "the reading is named elsewhere");
    let main = std::fs::read_to_string(sources.join("main.rs")).unwrap();
    let main: String = main.chars().filter(|c| !c.is_whitespace()).collect();
    let main = main.replace(",)", ")");
    // The models commands are handed the sign-in's place beside it, whose function
    // `scripts/check-sign-in.sh` holds to `main` (task N4).
    let platform = "std::sync::Arc::new(platforms_models_dir)";
    for handed in [
        format!("rascal::run(&command,{platform})"),
        format!("models::run(&command,{platform},"),
        format!("setup::run(&args,{platform},"),
        format!("backends::run(&command,{platform})"),
        format!("lab::run(&command,{platform})"),
        format!("serve::run(&args,{platform})"),
        format!("play::run(&args,{platform})"),
    ] {
        assert_eq!(main.matches(&handed).count(), 1, "{handed}");
    }
}

#[cfg(unix)]
#[test]
fn with_the_catalogue_it_ships_the_binary_opens_and_makes_no_models_directory() {
    // Plan 1C, task N5: the shipped catalogue holds one model, `sam3`, and every test
    // that starts the binary gives it a cleared environment with a temporary home
    // folder (task N3c). Until task P6a this build had no kind of backend, so no
    // config could offer a backend that asks for a model. **Changed by task P6a,**
    // which gives the build the kind `pack`: a command that builds an executor opens
    // a models directory only for a config that has a local backend, one the kind
    // `pack` takes as local: no url, no provider, no credential, under `segment`
    // (`wired::the_binary_opens_a_models_directory_only_for_a_config_that_has_a_local_backend`).
    // Under a config with none, empty or of remote backends alone, it opens none and
    // asks nothing of where the platform's is, whatever names one. Of the `models`
    // commands, only `models get` makes one
    // (`models::under_the_catalogue_it_ships_no_models_command_makes_a_models_directory_or_asks_anything_of_any_address`).
    let names: Vec<String> = wiring::catalogue()
        .unwrap()
        .entries()
        .iter()
        .map(|entry| entry.name().as_str().to_owned())
        .collect();
    // Plan 1D, task T1: and `tiny`.
    assert_eq!(names, ["sam3", "tiny"]);
    let kinds = wiring::kinds(&OneWorker::installed());
    assert!(kinds.get("pack").is_some(), "this build has the kind pack");
    let world = World::new("wired-shipped");
    std::fs::write(world.config_path(), "").unwrap();
    let (flagged, configured) = (world.beside("flagged"), world.beside("configured"));
    // Nothing is made in the home: no models directory in the platform's place.
    let run = |more: &[&str]| assert_eq!(grown_in_a_home(&world, more), Vec::new());
    // No directory named: the platform's would be in the temporary home.
    run(&[]);
    // The directory named by the flag, and then by the config.
    run(&["--models-dir", flagged.to_str().unwrap()]);
    let dir = format!("[models]\ndir = '{}'\n", configured.to_str().unwrap());
    std::fs::write(world.config_path(), &dir).unwrap();
    run(&[]);
    // A config whose backends are all remote: each names a url.
    let remote = "[backends.fill]\nkind = \"service\"\nurl = \"https://fill.example\"\n\n\
                  [backends.judge]\nkind = \"pack\"\nmodel = \"sam3\"\nurl = \"https://judge.example\"\n";
    std::fs::write(world.config_path(), format!("{remote}\n{dir}")).unwrap();
    run(&[]);
    run(&["--models-dir", flagged.to_str().unwrap()]);
    // Tables that name no url and that the kind `pack` refuses all the same: one that
    // names a provider, and a `pack` table under `fill`. The wiring asks the kind's own
    // rule, so neither opens a directory (from the review of task P6a: until then any
    // table with no url did).
    let refused = "[backends.segment]\nkind = \"acme\"\nprovider = \"acme\"\nmodel = \"sam3\"\n\n\
                   [backends.fill]\nkind = \"pack\"\nmodel = \"sam3\"\n";
    std::fs::write(world.config_path(), format!("{refused}\n{dir}")).unwrap();
    run(&[]);
    run(&["--models-dir", flagged.to_str().unwrap()]);
    assert!(!flagged.exists() && !configured.exists());
}

/// `whizbang rascal grow` of a mask the store does not hold, by the real binary, under
/// the world's config, with these arguments more: every folder it left in the
/// function's temporary home, which holds the platform's data folder, by its path
/// inside the home and with its permission bits. The command is kept until the run
/// has been waited for, and the home is looked into after it, before it is removed
/// with the command. The command gets as far as its store, and says nothing of a
/// models directory.
#[cfg(unix)]
fn grown_in_a_home(world: &World, more: &[&str]) -> Vec<(PathBuf, u32)> {
    let mask = format!("sha256:{}", "0".repeat(64));
    let mut command = whizbang_testkit::binary::command(env!("CARGO_BIN_EXE_whizbang"));
    let home = command.home().to_path_buf();
    let out = command
        .args(["rascal", "grow", "--mask", &mask, "--px", "1", "--config"])
        .arg(world.config_path())
        .args(more)
        .arg("--store")
        .arg(&world.root)
        .output()
        .expect("the binary runs");
    // The command got as far as its store: the mask is not in it.
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(!err.contains("models directory"), "{err}");
    let found = folders_under(&home).into_iter().map(|folder| {
        let mode = mode_of(&folder);
        (folder.strip_prefix(&home).unwrap().to_path_buf(), mode)
    });
    found.collect()
}

/// The folders under this one, to a depth of six, each by its path.
#[cfg(unix)]
fn folders_under(top: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut level = vec![top.to_path_buf()];
    for _ in 0..6 {
        let mut next = Vec::new();
        for folder in level {
            for entry in std::fs::read_dir(&folder).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    found.push(path.clone());
                    next.push(path);
                }
            }
        }
        level = next;
    }
    found
}

#[cfg(unix)]
#[test]
fn the_binary_opens_a_models_directory_only_for_a_config_that_has_a_local_backend() {
    // Plan 1C, task P6a, and the row of `docs/carry-over.md` on `rascal grow` opening a
    // models directory: with the kind `pack` in the build, a config that has a local
    // backend, one that names no url, opens the models directory where the executor
    // is built, and makes it where it is not, so that a refusal of the folder has a
    // place to be said. A config with none opens none
    // (`wired::with_the_catalogue_it_ships_the_binary_opens_and_makes_no_models_directory`).
    // Whether a backend is local is read from its table, not from its kind, so it does
    // not hang on which kinds were given first.
    let world = World::new("wired-local-opens");
    let local = "[backends.segment]\nkind = \"pack\"\nmodel = \"sam3\"\n\n\
                 [backends.segment.settings]\nmax_lifetime_s = 60\n";
    std::fs::write(world.config_path(), local).unwrap();
    let (flagged, configured) = (world.beside("flagged"), world.beside("configured"));
    // No worker is started: `grow` needs no backend.
    let run = |more: &[&str]| -> Vec<(PathBuf, u32)> {
        let made = grown_in_a_home(&world, more).into_iter();
        made.filter(|(folder, _)| folder.ends_with("whizbang/models"))
            .collect()
    };
    // No directory named: the platform's, in the temporary home, is made, for its
    // owner alone.
    let made = run(&[]);
    assert_eq!(made.len(), 1, "{made:?}");
    assert_eq!(made[0].1, 0o700);
    // Named by the flag, and then by the config: that one is made, and nothing in the
    // home.
    assert!(run(&["--models-dir", flagged.to_str().unwrap()]).is_empty());
    assert!(flagged.is_dir());
    let dir = format!(
        "{local}\n[models]\ndir = '{}'\n",
        configured.to_str().unwrap()
    );
    std::fs::write(world.config_path(), dir).unwrap();
    assert!(run(&[]).is_empty());
    assert!(configured.is_dir());
}

#[test]
fn a_fill_to_a_recipient_the_config_does_not_name_is_refused_before_a_byte_leaves() {
    // fal's filler, as a judge is above: a config that approves no recipient, then one
    // that approves another origin, then fal's origin for prompts alone, which are not
    // what a fill sends. The gate refuses each request before anything is handed to the
    // sender, and the server sees no connection.
    use crate::fal::Filled;
    let filled = Filled::new("wired-fill-unnamed");
    let listener = whizbang_testkit::loopback::Listener::serve(1, |_, _| {});
    let origin = format!("https://127.0.0.1:{}", listener.port());
    let address = format!("{origin}/fal-ai/bria/eraser");
    for approved in [
        None,
        Some("https://api.example.com".to_owned()),
        Some(origin.clone()),
    ] {
        let mut over = filled.over(&address, approved.as_deref());
        if approved.as_deref() == Some(origin.as_str()) {
            let config = std::fs::read_to_string(filled.world.config_path()).unwrap();
            let config = config.replace("[\"source_images\", \"derived_images\"]", "[\"prompts\"]");
            std::fs::write(filled.world.config_path(), config).unwrap();
            over = filled.over_written(&address);
        }
        let executor = filled.runner(&over);
        let refused = filled.fill(&executor);
        assert_eq!(refused.outcome().name(), "error", "{:?}", refused.outcome());
        drop(executor);
        let text = shown(&filled.world.root, refused.run().as_str());
        assert!(text.contains("\trequest_not_approved\t"), "{text}");
    }
    assert_eq!(listener.finish(), 0);
    assert!(!filled.store_holds_the_key());
}

#[test]
fn a_recipient_with_upstreams_is_sent_to_and_one_without_is_refused_naming_upstreams() {
    // Plan m3, task O5 (the row on a recipient approved with `upstreams`): the config
    // approves the origin with one upstream. A backend whose recipient forwards to that
    // upstream is the recipient approved and is called; one that declares the origin
    // alone is another recipient, refused before its backend is called, in the words
    // for upstreams.
    let world = World::new("wired-upstreams");
    reset(&world.root, "20");
    let config = |declared: &str| {
        format!(
            "[backends.fill]\nkind = \"fake\"\nurl = \"https://api.example.com/v1\"\n\n\
             [backends.fill.settings]\nat_most_micros = 250000\n{declared}\n\
             [egress.recipients.api]\norigin = \"https://api.example.com\"\n\
             upstreams = [\"https://up.example.com\"]\nclasses = [\"source_images\"]\n"
        )
    };
    let executor = world.runner(&world.config(&config("upstream = \"https://up.example.com\"\n")));
    let sent = executor.submit(&Call::new("paints")).unwrap();
    assert_eq!(sent.outcome().name(), "completed");
    assert_eq!((world.ran(), world.fakes.requests()), (1, 1));
    drop(executor);
    let text = shown(&world.root, sent.run().as_str());
    assert!(!text.contains("request_not_approved"), "{text}");
    let executor = world.runner(&world.config(&config("")));
    let refused = executor.submit(&Call::new("paints")).unwrap();
    assert_eq!(refused.outcome().name(), "error");
    assert_eq!((world.ran(), world.fakes.requests()), (2, 1));
    drop(executor);
    let text = shown(&world.root, refused.run().as_str());
    assert!(text.contains("\trequest_not_approved\t"), "{text}");
    assert!(
        text.contains("but not with the upstreams the backend declares"),
        "{text}"
    );
}

// --- the secrets told to the inventory at every ask (plan m6, task S1, item 6) ----------------

/// A world whose model is named by a made-up key, installed, with a config whose default
/// fill is that local model and whose other fill backend names the variable `FAKE_KEY`
/// of the env file, which holds `env`: what a command is built over, or why not.
fn a_model_named_by_a_key(world: &World, key: &str, env: &str) -> Result<Over, String> {
    let none = Secrets::none();
    let (named, downloads) = entry_named(key, "2026-11.1");
    let dir = ModelsDir::at(Arc::new(RealFs), &world.models_dir());
    install(
        &dir,
        &named,
        &adapter(),
        &downloads,
        &Cancellation::new(),
        &none,
    )
    .unwrap();
    let adapters = vec![(named.adapter().clone(), adapter())];
    let models = Models::of(Catalogue::new(vec![named], &none).unwrap(), adapters)
        .flag(Some(&world.models_dir()));
    let text = format!(
        "[backends.fill]\ndefault = \"local\"\n\n[backends.fill.local]\nkind = \"fake\"\n\
         model = \"{key}\"\n\n[backends.fill.paid]\nkind = \"fake\"\nkey_env = \"FAKE_KEY\"\n"
    );
    std::fs::write(world.config_path(), text).unwrap();
    std::fs::write(world.env_path(), format!("FAKE_KEY={env}\n")).unwrap();
    world
        .named()
        .env_file(Some(&world.env_path()))
        .installed(models)
        .map_err(|refused| refused.to_string())
}

/// Why the call is not prepared ready, when it is not: its refusal, or its reason.
fn not_ready_why(executor: &Engine) -> Option<String> {
    match executor.prepare(&Call::new("paints")) {
        Err(refused) => Some(refused.to_string()),
        Ok(prepared) => match prepared.readiness() {
            Readiness::Ready => None,
            Readiness::NotReady(missing) => Some(missing[0].why().to_owned()),
        },
    }
}

#[test]
fn a_key_given_at_the_start_is_refused_as_a_models_name() {
    let world = World::new("wired-key-at-start");
    let key = made_up_key("start");
    // The env file holds the key from the start. The command is built (no env file is
    // read to build it); the call that would use the model the key names is refused
    // when it is prepared and when it is sent, nothing of the key is said, and nothing
    // runs. (`contract::inventory::a_roster_told_the_snapshots_secrets_never_asks_…`
    // holds the inventory's own part, where no config reader stands before it.)
    let over = a_model_named_by_a_key(&world, &key, &key).unwrap();
    let executor = world.runner(&over);
    let why = not_ready_why(&executor).expect("not ready");
    assert!(!why.contains(&key), "{why}");
    let refused = executor
        .submit(&Call::new("paints"))
        .map(|_| ())
        .unwrap_err();
    assert!(!refused.to_string().contains(&key), "{refused}");
    assert_eq!(world.ran(), 0);
}

#[test]
fn a_key_added_to_the_env_file_after_the_start_is_refused_as_a_models_name_at_the_next_ask() {
    let world = World::new("wired-key-added");
    let key = made_up_key("added");
    let other = made_up_key("other");
    // At the start the env file holds another key: the model is found, and ready.
    let over = a_model_named_by_a_key(&world, &key, &other).unwrap();
    let executor = world.runner(&over);
    assert_eq!(not_ready_why(&executor), None);
    // The key is added to the env file. At the next ask the snapshot knows it, and the
    // call is refused: the config that names it is, and the inventory is told it with
    // the snapshot's other secrets, so the name goes to no model either way. Nothing
    // of the key is said, and nothing runs.
    std::fs::write(world.env_path(), format!("FAKE_KEY={key}\n")).unwrap();
    let why = not_ready_why(&executor).expect("not ready");
    assert!(!why.contains(&key), "{why}");
    assert!(executor.submit(&Call::new("paints")).is_err());
    assert_eq!(world.ran(), 0);
}
