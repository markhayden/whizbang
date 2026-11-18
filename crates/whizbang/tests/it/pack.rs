//! The kind `pack` through the binary's wiring (plan 1C, task P6a): a local segmenter
//! over the one worker of a process, with the pack's fake models in a test's catalogue,
//! installed in a models directory of the test's own, and test actions that ask for the
//! segment capability.
//!
//! **Every worker here is started through the test kit's guard** (plan 1C, "Rules"):
//! the one worker of the wiring is handed a start that runs the pack's fakes, by the
//! seam `OneWorker::starting`, which the binary never names
//! (`pack::the_shipped_binary_offers_no_fake_model_and_starts_no_entry_point_that_takes_a_fault`).
//! The guard kills it and waits for it on every path out of a test. Every config that
//! may start one sets `max_lifetime_s`, so that a worker nobody waits for ends by
//! itself.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use whizbang::pack::{self as kind, OneWorker};
use whizbang::wiring::{self, Models, Over};
use whizbang_backends::models::{
    Catalogue, CatalogueFile, Entry, EntryParts, FileName, Licence, ModelName, ModelsDir, Platform,
    Revision, Title, install,
};
use whizbang_backends::pack::queue::{Sent, Status};
use whizbang_backends::pack::worker::WORKER_MODULE;
use whizbang_backends::pack::{Launch, Worker};
use whizbang_core::capability::{FaultKind, Picture, Query, SegmentRequest};
use whizbang_core::contract::{
    Action, ActionDescriptor, Answer, Argument, Call, CallError, Cancellation, Capability, Context,
    Engine, Executor, Failure, Kind, PreparedCall, Produced, Readiness, Refusal, Registry, Request,
    Requirement, Secrets, SystemClock, Value, ValueType,
};
use whizbang_core::egress::{Address, Origin};
use whizbang_core::identity::{CanonicalPixels, Digest};
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;
use whizbang_testkit::downloads::FakeDownloader;
use whizbang_testkit::echo::Echo;
use whizbang_testkit::scratch::Scratch;
use whizbang_testkit::worker::{self as kit, Guard};

use crate::wired::{has_line, shown, whizbang};

/// The model every config here asks for unless it says another: the fake that answers
/// with the masks its request determines.
const OK: &str = "fake-ok";
/// The fake that waits until its call is cancelled.
const WAIT: &str = "fake-wait";
/// The revision the adapter of the tests' entries states of itself.
const ADAPTER_REVISION: &str = "7";

/// What one call of a test action came to: the instances of its one query, each as its
/// plane, its box and its score; or the fault's kind and sentence.
type Came = Result<Vec<(Vec<u8>, [f64; 4], f64)>, (FaultKind, String)>;

/// The catalogue's entry for a fake model: one file, and the features the pack's fake
/// says it gives.
fn entry(model: &str, features: &[&str]) -> (Entry, FakeDownloader) {
    let none = Secrets::none();
    let name = |text: &str| ModelName::new(text, &none).unwrap();
    let content = format!("weights of {model}").into_bytes();
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
        title: Title::new("A fake model of the pack", &none).unwrap(),
        revision: Revision::new("2026-11.1", &none).unwrap(),
        adapter: name(model),
        licence: Licence::new("Apache-2.0", &none).unwrap(),
        platforms: Platform::ALL.to_vec(),
        features: features
            .iter()
            .map(|feature| (*feature).to_owned())
            .collect(),
        origins: vec![Origin::of("https://weights.example.org", &none).unwrap()],
        files: vec![file.unwrap()],
    };
    (
        Entry::new(parts, &none).unwrap(),
        FakeDownloader::new().serving(&from, content),
    )
}

/// The two fake models of the tests' catalogue.
fn entries() -> Vec<(Entry, FakeDownloader)> {
    vec![
        entry(OK, &["box", "phrase", "point"]),
        entry(WAIT, &["phrase"]),
    ]
}

fn adapter_revision() -> Revision {
    Revision::new(ADAPTER_REVISION, &Secrets::none()).unwrap()
}

/// The one worker of the wiring, started from the pack's fakes, held by the test kit's
/// guard: its drop kills the worker and waits for it.
fn guarded() -> Guard<Arc<OneWorker>> {
    kit::fakes(None).guard(
        |fixture| {
            let (python, source) = (fixture.python.clone(), fixture.source.clone());
            let (module, more) = (fixture.module.clone(), fixture.more.clone());
            let lifetime_s = fixture.lifetime_s;
            OneWorker::starting(Arc::new(move |root: &Path, secrets: &Secrets| {
                let launch = Launch {
                    python: python.clone(),
                    source: source.clone(),
                    module: module.clone(),
                    models_dir: root.to_path_buf(),
                    lifetime_s,
                    more: more.clone(),
                };
                let clock = Arc::new(SystemClock);
                Ok(Worker::new(launch, clock, secrets.clone()).with_spawn(kit::spawn))
            }))
        },
        |worker| {
            worker.kill();
        },
    )
}

/// A picture of four by two pixels whose red is 128 or more at three of them: what the
/// fake `fake-ok` finds for a phrase.
fn picture() -> Picture {
    let reds = [10u8, 200, 200, 10, 10, 10, 200, 10];
    let rgba = reds.iter().flat_map(|red| [*red, 7, 7, 255]).collect();
    Picture::new(CanonicalPixels::from_rgba(4, 2, rgba).unwrap()).unwrap()
}

/// What `fake-ok` answers a phrase about [`picture`] with: one instance, the three
/// pixels, their box, and a score of 0.7.
fn determined() -> Vec<(Vec<u8>, [f64; 4], f64)> {
    let plane = vec![0, 255, 255, 0, 0, 0, 255, 0];
    vec![(plane, [1.0, 0.0, 3.0, 2.0], 0.7)]
}

/// An action of this name that needs a segmenter for phrases, chosen by its argument
/// `backend` when the call gives one, and asks it once about [`picture`]. What it came
/// to is kept in `came`; a fault fails the run, as an error of the kind `segment`.
fn finds(name: &str, came: &Arc<Mutex<Vec<Came>>>) -> Arc<dyn Action> {
    struct Finds(ActionDescriptor, Arc<Mutex<Vec<Came>>>);
    impl Action for Finds {
        fn descriptor(&self) -> &ActionDescriptor {
            &self.0
        }
        fn run(&self, _: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
            let segment = context
                .capabilities()
                .segment()
                .expect("a segmenter is handed");
            let query = Query::phrase("the red part").unwrap();
            let request = SegmentRequest::new(picture(), vec![query], 4).unwrap();
            let came: Came = match segment.segment(&request) {
                Ok(response) => Ok(response.answers()[0]
                    .iter()
                    .map(|found| (found.plane().to_vec(), found.bounds(), found.score()))
                    .collect()),
                Err(fault) => Err((fault.kind(), fault.sentence().to_owned())),
            };
            self.1.lock().unwrap().push(came.clone());
            match came {
                Ok(_) => Ok(Produced::nothing()),
                Err((_, sentence)) => Err(Failure::error("segment", &sentence)),
            }
        }
    }
    let need = Requirement::new(Capability::Segment)
        .feature("phrase")
        .chosen_by("backend");
    let draft = ActionDescriptor::draft(name, "1.0.0", Kind::Primitive, "A test action.")
        .argument(Argument::optional(
            "backend",
            "The segmenter.",
            ValueType::Text,
        ))
        .needs(need);
    Arc::new(Finds(draft.build().unwrap(), came.clone()))
}

/// A scratch folder with a store, a config file, and a models directory in which the
/// tests' two fake models are installed; and the one worker of the wiring, in its guard.
struct Local {
    /// First, so that it is dropped first: the worker is killed and waited for before
    /// the folders go.
    worker: Guard<Arc<OneWorker>>,
    dir: Scratch,
    root: PathBuf,
    came: Arc<Mutex<Vec<Came>>>,
}

impl Local {
    fn new(label: &str) -> Local {
        let dir = Scratch::new(label);
        let root = dir.path().join("store");
        drop(Store::open(&root, Arc::new(RealFs)).unwrap());
        let models = dir.path().join("models");
        std::fs::create_dir(&models).unwrap();
        let local = Local {
            worker: guarded(),
            dir,
            root,
            came: Arc::default(),
        };
        let at = ModelsDir::at(Arc::new(RealFs), &models);
        for (entry, downloads) in entries() {
            let none = Secrets::none();
            let cancel = Cancellation::new();
            install(&at, &entry, &adapter_revision(), &downloads, &cancel, &none).unwrap();
        }
        local
    }

    fn models_dir(&self) -> PathBuf {
        self.dir.path().join("models")
    }

    fn config_path(&self) -> PathBuf {
        self.dir.path().join("whizbang.toml")
    }

    /// The tests' catalogue under the adapters of its models, with the flag naming this
    /// folder's models directory.
    fn models(&self) -> Models {
        let none = Secrets::none();
        let (entries, adapters): (Vec<Entry>, Vec<(ModelName, Revision)>) = entries()
            .into_iter()
            .map(|(entry, _)| {
                let adapter = (entry.adapter().clone(), adapter_revision());
                (entry, adapter)
            })
            .unzip();
        Models::of(Catalogue::new(entries, &none).unwrap(), adapters).flag(Some(&self.models_dir()))
    }

    /// Writes the config, and answers what the binary's wiring builds over it: the
    /// kinds of this build over the one worker, and the tests' models in the
    /// directory the wiring opened.
    fn over(&self, text: &str) -> Over {
        std::fs::write(self.config_path(), text).unwrap();
        Over::nothing()
            .config(Some(&self.config_path()))
            .kinds(wiring::kinds(&self.worker))
            .pack(self.worker.clone())
            .installed(self.models())
            .expect("the models directory is opened")
    }

    /// The actions of this build, with `finds`, `finds-again` and `echo` beside them.
    fn registry(&self) -> Registry {
        let mut registry = wiring::registry().unwrap();
        registry.register(finds("finds", &self.came)).unwrap();
        registry.register(finds("finds-again", &self.came)).unwrap();
        registry.register(Arc::new(Echo::new())).unwrap();
        registry
    }

    /// The binary's executor over this folder's store, built over this.
    fn runner(&self, over: &Over) -> Engine {
        let store = Arc::new(Store::open(&self.root, Arc::new(RealFs)).unwrap());
        wiring::runner(self.registry(), store, over).expect("the executor builds")
    }

    /// What the test actions came to, oldest first, taken.
    fn came(&self) -> Vec<Came> {
        std::mem::take(&mut *self.came.lock().unwrap())
    }

    /// The identity of this model's installation, as the inventory gives it.
    fn identity(&self, over: &Over, model: &str) -> String {
        let record = over.installations().unwrap().ready(model).unwrap();
        record.identity().digest().to_string()
    }

    /// The requests written to a worker, as `(kind, model, worker)`.
    fn journal(&self) -> Vec<(&'static str, String, u32)> {
        let sent: Vec<Sent> = self.worker.journal();
        sent.into_iter()
            .map(|sent| (sent.kind, sent.model.as_str().to_owned(), sent.worker))
            .collect()
    }
}

/// A config of one backend of the kind `pack`, the capability's default, for this
/// model, with these lines of settings beside the lifetime every config here sets.
fn one(model: &str, settings: &str) -> String {
    format!(
        "[backends.segment]\nkind = \"pack\"\nmodel = \"{model}\"\n\n\
         [backends.segment.settings]\nmax_lifetime_s = 120\n{settings}"
    )
}

/// The reason a call that resolved the segment backend was refused as not ready.
fn not_ready(refused: Result<impl std::fmt::Debug, CallError>) -> String {
    match refused {
        Err(CallError::Refused(Refusal::NotReady { missing, .. })) => {
            assert_eq!(missing.len(), 1, "{missing:?}");
            assert_eq!(missing[0].capability(), Capability::Segment);
            missing[0].why().to_owned()
        }
        other => panic!("not refused as not ready: {other:?}"),
    }
}

/// The command line of the process with this id, as `ps` lists it.
fn command_line(pid: u32) -> String {
    let listed = kit::processes()
        .into_iter()
        .find(|listed| listed.pid == pid);
    listed.expect("the worker runs").command
}

/// The worker's process id, as the supervisor says.
fn running(worker: &OneWorker) -> u32 {
    let status: Status = worker.status().expect("a supervisor was made");
    status.worker.expect("a worker runs")
}

// --- a call, answered by the worker ------------------------------------------------------------

#[test]
fn a_call_to_a_local_model_is_answered_by_the_worker_and_its_record_names_the_backend_the_model_and_the_installation()
 {
    let local = Local::new("pack-answered");
    let over = local.over(&one(OK, ""));
    let identity = local.identity(&over, OK);
    let executor = local.runner(&over);
    let finished = executor.submit(&Call::new("finds")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    // The masks the fake determines from the request, through the worker and back.
    assert_eq!(local.came(), [Ok(determined())]);
    let pid = running(&local.worker);
    assert!(kit::runs_the_pack(pid));
    assert_eq!(
        local.journal(),
        [
            ("load", OK.to_owned(), pid),
            ("segment", OK.to_owned(), pid)
        ]
    );
    drop(executor);
    // The record names the backend, the model and the installation, whole.
    let json = whizbang(
        &["runs", "show", finished.run().as_str(), "--json"],
        &local.root,
    );
    assert_eq!(json.code, Some(0), "{}", json.err);
    let record: serde_json::Value = serde_json::from_str(&json.out).unwrap();
    let models = record["models"].as_array().unwrap();
    assert_eq!(models.len(), 1, "{models:?}");
    assert_eq!(models[0]["capability"], "segment");
    assert_eq!(models[0]["backend"], kind::KIND);
    assert_eq!(models[0]["model"], OK);
    assert_eq!(models[0]["version"], identity.as_str());
    // And the settings the run recorded: the kind and the model of the backend.
    let text = shown(&local.root, finished.run().as_str());
    has_line(&text, "  backends.segment.default.kind = \"pack\"");
    has_line(&text, "  backends.segment.default.model = \"fake-ok\"");
}

#[test]
fn two_actions_in_one_process_that_use_one_model_cause_one_load() {
    let local = Local::new("pack-one-load");
    let over = local.over(&one(OK, ""));
    let executor = local.runner(&over);
    for action in ["finds", "finds-again", "finds"] {
        let finished = executor.submit(&Call::new(action)).unwrap();
        assert_eq!(finished.outcome().name(), "completed", "{action}");
    }
    assert_eq!(local.came(), vec![Ok(determined()); 3]);
    let pid = running(&local.worker);
    let segment = ("segment", OK.to_owned(), pid);
    assert_eq!(
        local.journal(),
        [
            ("load", OK.to_owned(), pid),
            segment.clone(),
            segment.clone(),
            segment
        ]
    );
}

#[test]
fn two_backends_of_the_kind_under_two_names_and_two_snapshots_of_the_config_share_one_worker() {
    let local = Local::new("pack-shared");
    let two = |pad: &str| {
        format!(
            "[backends.segment]\ndefault = \"a\"\n\n\
             [backends.segment.a]\nkind = \"pack\"\nmodel = \"fake-ok\"\n\n\
             [backends.segment.a.settings]\nmax_lifetime_s = 120\n\n\
             [backends.segment.b]\nkind = \"pack\"\nmodel = \"fake-ok\"\n\n\
             [backends.segment.b.settings]\nmax_lifetime_s = 120\npad = \"{pad}\"\n"
        )
    };
    let over = local.over(&two("0.05"));
    let executor = local.runner(&over);
    let call = |backend: Option<&str>| {
        let call = Call::new("finds");
        let call = match backend {
            Some(name) => call.argument("backend", Value::text(name)),
            None => call,
        };
        let finished = executor.submit(&call).unwrap();
        assert_eq!(finished.outcome().name(), "completed", "{backend:?}");
    };
    // The default, `a`; then `b`, whose settings differ, so its model is loaded again
    // with them; then `a` again.
    call(None);
    call(Some("b"));
    call(Some("a"));
    // The config is written again: the next call is under another snapshot of it,
    // whose backends were made again by the kind. The same worker answers.
    std::fs::write(local.config_path(), two("0.1")).unwrap();
    call(Some("b"));
    assert_eq!(local.came(), vec![Ok(determined()); 4]);
    let pid = running(&local.worker);
    let journal = local.journal();
    let kinds: Vec<&str> = journal.iter().map(|(kind, _, _)| *kind).collect();
    assert_eq!(
        kinds,
        [
            "load", "segment", "unload", "load", "segment", "unload", "load", "segment", "unload",
            "load", "segment"
        ]
    );
    // One worker answered every one of them: one process, one supervisor.
    assert!(
        journal.iter().all(|(_, _, worker)| *worker == pid),
        "{journal:?}"
    );
    assert_eq!(kit::pack_children(), [pid]);
    // And the one worker is told one models directory: once its supervisor runs over
    // this one, the wiring is refused another, whichever of the two it is given first.
    drop(executor);
    let other = local.dir.path().join("other-models");
    std::fs::create_dir(&other).unwrap();
    let elsewhere = || local.models().flag(Some(&other));
    let refused = Over::nothing()
        .config(Some(&local.config_path()))
        .kinds(wiring::kinds(&local.worker))
        .pack(local.worker.clone())
        .installed(elsewhere())
        .unwrap_err();
    assert!(refused.contains("one process has one worker"), "{refused}");
    let over = Over::nothing()
        .config(Some(&local.config_path()))
        .installed(elsewhere())
        .unwrap()
        .kinds(wiring::kinds(&local.worker))
        .pack(local.worker.clone());
    let store = Arc::new(Store::open(&local.root, Arc::new(RealFs)).unwrap());
    let Err(refused) = wiring::runner(local.registry(), store, &over) else {
        panic!("the executor was built");
    };
    assert!(refused.contains("one process has one worker"), "{refused}");
}

#[test]
fn a_run_that_needs_no_local_model_starts_no_worker() {
    let local = Local::new("pack-not-started");
    let over = local.over(&one(OK, ""));
    let executor = local.runner(&over);
    // The config has a local backend, and the call needs no backend at all.
    let call = Call::new("echo").argument("text", Value::text("hello"));
    let finished = executor.submit(&call).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    // And a call that needs one, prepared and not run.
    let prepared = executor.prepare(&Call::new("finds")).unwrap();
    assert_eq!(prepared.readiness(), &Readiness::Ready);
    // No supervisor was made, and no worker started.
    assert!(local.worker.status().is_none());
    assert!(kit::pack_children().is_empty());
    assert!(local.journal().is_empty());
}

#[test]
fn a_worker_started_through_the_wiring_is_given_the_lifetime_its_config_sets() {
    let local = Local::new("pack-lifetime");
    let lifetime = |seconds: &str| {
        format!(
            "[backends.segment]\nkind = \"pack\"\nmodel = \"fake-ok\"\n\n\
             [backends.segment.settings]\nmax_lifetime_s = {seconds}\n"
        )
    };
    // Out of its bound, the setting is refused, and no worker is started.
    for refused in ["0", "-1", "31536001", "\"60\""] {
        let executor = local.runner(&local.over(&lifetime(refused)));
        let why = not_ready(executor.submit(&Call::new("finds")));
        assert!(why.ends_with(kind::BAD_LIFETIME), "{refused}: {why}");
        assert!(local.worker.status().is_none(), "{refused}");
    }
    // Within it, the worker the call starts is given it, and the models directory the
    // wiring opened: the one the supervisor reads the counts of removals in.
    let executor = local.runner(&local.over(&lifetime("97")));
    let finished = executor.submit(&Call::new("finds")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    let line = command_line(running(&local.worker));
    assert!(line.contains(" --max-lifetime-s 97"), "{line}");
    let dir = local.models_dir();
    let dir = format!(" --models-dir {}", dir.to_str().unwrap());
    assert!(line.contains(&dir), "{line}");
}

#[test]
fn a_model_the_config_resolves_and_no_backend_holds_is_refused_at_admission() {
    let local = Local::new("pack-unheld");
    // Two backends of the kind, made in one walk of the config: `a`, whose model is
    // installed, and `b`, whose model the catalogue does not hold, and `c`, which the
    // kind does not take. A call that chooses `a` is handed the backend that walk made
    // for `a`, and the run names that backend's model. One that chooses `b` or `c`
    // resolves no model the snapshot holds a ready backend for: it is refused at its
    // admission, with no run, and no worker is started for it.
    let config = "[backends.segment]\ndefault = \"a\"\n\n\
                  [backends.segment.a]\nkind = \"pack\"\nmodel = \"fake-ok\"\n\n\
                  [backends.segment.a.settings]\nmax_lifetime_s = 120\n\n\
                  [backends.segment.b]\nkind = \"pack\"\nmodel = \"fake-none\"\n\n\
                  [backends.segment.b.settings]\nmax_lifetime_s = 120\n\n\
                  [backends.segment.c]\nkind = \"pack\"\nmodel = \"fake-ok\"\n\n\
                  [backends.segment.c.settings]\nmax_lifetime_s = 120\nleast = 1\n";
    let executor = local.runner(&local.over(config));
    let choosing = |name: &str| Call::new("finds").argument("backend", Value::text(name));
    let why = not_ready(executor.submit(&choosing("b")));
    assert!(why.ends_with(kind::NOT_IN_CATALOGUE), "{why}");
    let why = not_ready(executor.submit(&choosing("c")));
    assert!(why.ends_with(kind::UNKNOWN_SETTING), "{why}");
    let reader = whizbang_core::store::Reader::open(&local.root, Arc::new(RealFs)).unwrap();
    assert!(reader.snapshot().unwrap().runs().is_empty());
    assert!(local.worker.status().is_none());
    let finished = executor.submit(&choosing("a")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    assert_eq!(local.came(), [Ok(determined())]);
    let pid = running(&local.worker);
    assert_eq!(
        local.journal(),
        [
            ("load", OK.to_owned(), pid),
            ("segment", OK.to_owned(), pid)
        ]
    );
}

#[test]
fn a_setting_the_kind_does_not_know_is_refused_when_the_config_is_read() {
    let local = Local::new("pack-settings");
    // Each table the kind does not take, with the end of the reason a call is given.
    let refused = [
        (one(OK, "longest_side = 2048\n"), kind::UNKNOWN_SETTING),
        (one(OK, "threshold = \"0/2\"\n"), kind::BAD_TEXT),
        (one(OK, "outline = 'a\\b'\n"), kind::BAD_TEXT),
        (one(OK, "threshold = \"2\"\n"), kind::BAD_FRACTION),
        (one(OK, "pad = 1\n"), kind::BAD_FRACTION),
        (one(OK, "min_cover = \"nan\"\n"), kind::BAD_FRACTION),
        (one(OK, "outline = true\n"), kind::BAD_OUTLINE),
        (one(OK, "call_limit_s = 121\n"), kind::BAD_CALL_LIMIT),
        (one(OK, "call_limit_s = 0\n"), kind::BAD_CALL_LIMIT),
        (
            "[backends.segment]\nkind = \"pack\"\n".to_owned(),
            kind::NO_MODEL,
        ),
        (
            "[backends.segment]\nkind = \"pack\"\nmodel = \"fake-ok\"\nurl = \"https://x.example\"\n"
                .to_owned(),
            kind::NOT_LOCAL,
        ),
        (
            "[backends.segment]\nkind = \"pack\"\nmodel = \"Fake OK\"\n".to_owned(),
            kind::BAD_MODEL,
        ),
    ];
    for (text, end) in &refused {
        let over = local.over(text);
        // The config is read as a config: the reader takes it.
        wiring::loaded(&over).unwrap();
        // And each reading of it for a call shows the table to the kind, which refuses
        // it in its own words: the backend is not ready, and nothing is started.
        let executor = local.runner(&over);
        let prepared = executor.prepare(&Call::new("finds")).unwrap();
        let Readiness::NotReady(missing) = prepared.readiness() else {
            panic!("ready: {text}");
        };
        assert!(
            missing[0].why().ends_with(end),
            "{text}: {}",
            missing[0].why()
        );
        assert!(
            missing[0].why().starts_with(
                "the default segment backend is not ready: its kind does not take it: "
            ),
            "{}",
            missing[0].why()
        );
        assert!(not_ready(executor.submit(&Call::new("finds"))).ends_with(end));
    }
    // A table of another capability is refused too.
    let fill = "[backends.fill]\nkind = \"pack\"\nmodel = \"fake-ok\"\n";
    let offers = whizbang_core::config::Offers::of(
        &whizbang_core::config::Config::parse(fill.as_bytes(), &Secrets::none()).unwrap(),
        &whizbang_core::config::Credentials::none(),
        &wiring::kinds(&local.worker),
    )
    .unwrap();
    let why = offers.not_ready(Capability::Fill, "default").unwrap();
    assert!(why.to_string().ends_with(kind::NOT_SEGMENT), "{why}");
    assert!(local.worker.status().is_none());
    // Every setting the kind knows, each within its rule, is taken.
    let all = "call_limit_s = 120\nmin_cover = \"0.2\"\noutline = \"whole\"\npad = \"0.05\"\n\
               threshold = \"0.2\"\n";
    let executor = local.runner(&local.over(&one(OK, all)));
    let finished = executor.submit(&Call::new("finds")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
}

#[test]
fn a_local_call_reserves_nothing_and_adds_no_name_under_spend() {
    let local = Local::new("pack-free");
    let executor = local.runner(&local.over(&one(OK, "")));
    // The store has no allowance: a call that may cost anything would be refused.
    let finished = executor.submit(&Call::new("finds")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    assert_eq!(local.came(), [Ok(determined())]);
    drop(executor);
    let reader = whizbang_core::store::Reader::open(&local.root, Arc::new(RealFs)).unwrap();
    let generation = reader.snapshot().unwrap();
    let spend: Vec<&String> = generation
        .entries()
        .keys()
        .filter(|name| name.starts_with("spend/"))
        .collect();
    assert!(spend.is_empty(), "{spend:?}");
    has_line(
        &shown(&local.root, finished.run().as_str()),
        "spend: $0.00 reserved; paid requests: none; cost: $0.00",
    );
}

// --- the shipped binary -------------------------------------------------------------------------

/// Every source file of the binary's crate, by its name, as text.
fn the_binarys_sources() -> Vec<(String, String)> {
    let sources = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut read = Vec::new();
    for entry in std::fs::read_dir(&sources).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|extension| extension == "rs") {
            let name = path.file_name().unwrap().to_str().unwrap().to_owned();
            read.push((name, std::fs::read_to_string(&path).unwrap()));
        }
    }
    assert!(
        read.len() > 10,
        "{} files of the binary were read",
        read.len()
    );
    read
}

#[test]
fn the_shipped_binary_offers_no_fake_model_and_starts_no_entry_point_that_takes_a_fault() {
    // The catalogue it ships names no model of the pack's fakes.
    let shipped = wiring::catalogue().unwrap();
    let names: Vec<&str> = shipped
        .entries()
        .iter()
        .map(|entry| entry.name().as_str())
        .collect();
    assert_eq!(names, ["sam3"]);
    let fakes = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../pack/src/whizbang_pack/fake_models.py"
    ))
    .unwrap();
    for (name, _) in entries() {
        let name = name.name().as_str().to_owned();
        assert!(fakes.contains(&format!("\"{name}\": ")), "{name}");
        assert!(shipped.entry(&name).is_none(), "{name}");
    }
    // What the binary starts: the worker itself, from the installation `setup` made in
    // the models directory, with no fault and nothing more on its command line.
    let root = Path::new("/made-up/models");
    let launch = kind::installed_launch(root, 61);
    assert_eq!(launch.module, WORKER_MODULE);
    assert!(launch.more.is_empty(), "{launch:?}");
    assert_eq!(
        launch.python,
        root.join("pack").join("env").join("bin").join("python")
    );
    assert_eq!(launch.source, root.join("pack").join("project").join("src"));
    assert_eq!((launch.models_dir.as_path(), launch.lifetime_s), (root, 61));
    // And no source of the binary names the fakes' entry point or a fault, makes a
    // launch of its own, or hands the wiring a worker it did not start from that
    // installation: the one worker the binary makes is `OneWorker::installed`'s, and
    // the seam a test starts the fakes through is named only where it is defined.
    // The needles are put together, so that this file holds none of them.
    let entry_point = ["whizbang_pack", "fakes"].join(".");
    let literal = ["Launch", " {"].concat();
    let mut installed = Vec::new();
    for (name, text) in the_binarys_sources() {
        for word in [entry_point.as_str(), "--fake-fault", OK] {
            assert!(!text.contains(word), "{name} names {word}");
        }
        // A launch made field by field: what a function hands back is no such thing.
        let returned = text.matches(&format!("-> {literal}")).count();
        let made = text.matches(literal.as_str()).count() - returned;
        assert_eq!(made, 0, "{name} makes a launch of its own");
        let starting = text.matches("OneWorker::starting(").count();
        assert_eq!(starting, 0, "{name} hands the wiring a worker of its own");
        let defined = text.matches("pub fn starting(").count();
        assert!(defined == 0 || name == "pack.rs", "{name}");
        let launches = text.matches("Launch::worker(").count();
        let expected = match name.as_str() {
            "pack.rs" | "pack_setup.rs" => 1,
            _ => 0,
        };
        assert_eq!(launches, expected, "{name}");
        if text.contains("OneWorker::installed()") {
            installed.push(name);
        }
    }
    assert_eq!(installed, ["rascal.rs"]);
}

#[test]
fn without_setup_a_call_is_refused_and_says_to_run_whizbang_setup() {
    // The worker the binary makes, over a models directory in which `whizbang setup`
    // installed nothing: the call is ready, its model is installed, and the request
    // is refused before anything is started, saying what to do.
    let local = Local::new("pack-no-setup");
    let worker = OneWorker::installed();
    std::fs::write(local.config_path(), one(OK, "")).unwrap();
    let over = Over::nothing()
        .config(Some(&local.config_path()))
        .kinds(wiring::kinds(&worker))
        .pack(worker.clone())
        .installed(local.models())
        .unwrap();
    let executor = local.runner(&over);
    let finished = executor.submit(&Call::new("finds")).unwrap();
    assert_eq!(finished.outcome().name(), "error");
    let came = local.came();
    let Err((fault, sentence)) = &came[0] else {
        panic!("answered: {came:?}");
    };
    assert_eq!(*fault, FaultKind::Unavailable);
    assert!(sentence.contains("run whizbang setup"), "{sentence}");
    assert!(kit::pack_children().is_empty());
    assert!(worker.journal().is_empty());
}

// --- what bounds a request through the kind -----------------------------------------------------

#[test]
fn a_request_through_the_kind_is_bounded_by_its_runs_deadline_and_cancel_and_by_the_kinds_limit() {
    let local = Local::new("pack-bounded");
    let wait = |settings: &str| one(WAIT, settings);
    // The kind's limit for a call, a setting: a call past it ends as timed out, and its
    // worker is killed.
    let executor = local.runner(&local.over(&wait("call_limit_s = 1\n")));
    let begun = Instant::now();
    let finished = executor.submit(&Call::new("finds")).unwrap();
    assert_eq!(finished.outcome().name(), "error");
    assert!(begun.elapsed() >= Duration::from_secs(1));
    let came = local.came();
    assert!(
        matches!(&came[..], [Err((FaultKind::TimedOut, _))]),
        "{came:?}"
    );
    drop(executor);
    // The run's deadline, handed to the queue: the call ends at it, as timed out.
    let executor = local.runner(&local.over(&wait("")));
    let request = Request::new(Call::new("finds")).until(Instant::now() + Duration::from_secs(1));
    let ran = executor.request(&request).unwrap();
    assert!(matches!(ran, Answer::Ran(_)), "{ran:?}");
    let came = local.came();
    assert!(
        matches!(&came[..], [Err((FaultKind::TimedOut, _))]),
        "{came:?}"
    );
    // The run's cancel, handed to the queue: the worker is sent `cancel` once the call
    // is written, the call ends as cancelled, and the worker lives on.
    let signal = Cancellation::new();
    let (canceller, worker) = (signal.clone(), local.worker.clone());
    let cancels = whizbang_testkit::threads::spawn(move || {
        let written = |status: &Status| {
            status
                .calling
                .as_ref()
                .is_some_and(|calling| calling.kind == "segment" && calling.written)
        };
        let seen = worker.wait_for(Duration::from_secs(20), &written);
        canceller.cancel();
        seen
    });
    let request = Request::new(Call::new("finds")).cancelled_by(signal);
    executor.request(&request).unwrap();
    assert!(cancels.join().unwrap(), "the call was never seen written");
    let came = local.came();
    assert!(
        matches!(&came[..], [Err((FaultKind::Cancelled, _))]),
        "{came:?}"
    );
    let pid = running(&local.worker);
    assert!(kit::runs_the_pack(pid));
}

// --- a removal in this process ------------------------------------------------------------------

#[test]
fn a_removal_in_this_process_through_the_wiring_tells_the_supervisor_and_unloads_the_resident_model()
 {
    let local = Local::new("pack-removed");
    let over = local.over(&one(OK, ""));
    let executor = local.runner(&over);
    let finished = executor.submit(&Call::new("finds")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    let pid = running(&local.worker);
    let resident = local.worker.status().unwrap().resident;
    assert_eq!(
        resident.map(|model| model.as_str().to_owned()),
        Some(OK.to_owned())
    );
    // The removal goes through the one worker, which tells the supervisor once the
    // mark is set: the model, resident and idle, is unloaded at once.
    let model = ModelName::new(OK, &Secrets::none()).unwrap();
    let removed = local
        .worker
        .remove(&model, Duration::from_secs(5), &Secrets::none())
        .unwrap();
    assert!(removed.installation().is_some());
    assert_eq!(local.worker.status().unwrap().resident, None);
    assert_eq!(
        local.journal(),
        [
            ("load", OK.to_owned(), pid),
            ("segment", OK.to_owned(), pid),
            ("unload", OK.to_owned(), pid)
        ]
    );
    // And the model is gone: the next call for it is not ready.
    let why = not_ready(executor.submit(&Call::new("finds")));
    assert!(why.ends_with("it is not installed"), "{why}");
}
