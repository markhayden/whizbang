//! What the tests of `check` run in: a store on the test kit's disk, an executor that
//! holds `check`, `view`, `align`, `resize`, a stand-in for rascal's `grow`, and a scripted judge read
//! from data, and readers of what a run left in the store.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value as Json;
use whizbang_core::artifact::{self, Artifact, ArtifactId, ArtifactType, Image};
use whizbang_core::capability::Judge;
use whizbang_core::contract::record::RunRecord;
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Backends, Call, Capabilities, Capability, Clock, Context,
    Engine, Estimate, Failure, InputPort, Kind, ModelIdentity, PreparedCall, Produced,
    RUN_RECORD_SCHEMA, Registry, RunId, Secrets, Unavailable, ValueType,
};
use whizbang_core::identity::Digest;
use whizbang_core::store::fs::Fs;
use whizbang_core::store::{HOLDS_PREFIX, Kind as Stored, Reader, Store};
use whizbang_squint::align::Align;
use whizbang_squint::check::{Check, Toolbox};
use whizbang_squint::compare::Compare;
use whizbang_squint::resize::Resize;
use whizbang_squint::view::View;
use whizbang_testkit::capabilities::{FakeJudge, roster};
use whizbang_testkit::fs_shim::ShimFs;
use whizbang_testkit::schema::validate;
use whizbang_testkit::scratch::Scratch;

use super::support::*;

/// A clock that says one millisecond more each time it is asked.
struct Ticking(AtomicU64);

impl Clock for Ticking {
    fn now_ms(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst)
    }
}

/// A stand-in for rascal's `grow`: an action of that name that takes a mask's place
/// and a number, and counts each time it runs. No judge may make it run.
pub struct Grow(ActionDescriptor, pub Arc<AtomicU64>);

impl Action for Grow {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }

    fn run(&self, _: &PreparedCall, _: &Context) -> Result<Produced, Failure> {
        self.1.fetch_add(1, Ordering::SeqCst);
        Ok(Produced::nothing())
    }
}

/// The roster of a world, whose every model states one bound on what a call of it may
/// cost, in millionths of a US dollar.
struct Priced(whizbang_core::capability::Roster, u64);

impl Backends for Priced {
    fn resolve(
        &self,
        capability: Capability,
        choice: Option<&str>,
        features: &[String],
    ) -> Result<ModelIdentity, Unavailable> {
        Backends::resolve(&self.0, capability, choice, features)
    }

    fn bound(&self, _: Capability, _: &ModelIdentity) -> Estimate {
        Estimate::of_micros(self.1)
    }
}

/// An action that is another, with something done first each time it runs: how a test
/// reaches the moment after a call was admitted and before its action reads anything.
pub struct Before(pub Arc<dyn Action>, pub Box<dyn Fn() + Send + Sync>);

impl Action for Before {
    fn descriptor(&self) -> &ActionDescriptor {
        self.0.descriptor()
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        (self.1)();
        self.0.run(call, context)
    }
}

pub struct World {
    dir: Scratch,
    pub fs: Arc<ShimFs>,
    /// The photograph every run is given, and one the store holds that no run is.
    pub photo: Image,
    pub other: Image,
    pub toolbox: Arc<Toolbox>,
    /// How many times the stand-in for `grow` ran.
    pub grown: Arc<AtomicU64>,
    /// Whether `check` reads and keeps the decision cache.
    pub cached: bool,
    /// The text of each use case's recipe the executor registers: the shipped ones,
    /// unless a test changes one.
    pub recipes: Vec<String>,
    /// More actions the executor registers before the recipes, which a test's recipe
    /// may name.
    pub more: Vec<Arc<dyn Action>>,
}

impl World {
    pub fn new() -> World {
        World::on(ShimFs::new())
    }

    /// A world whose store holds both photographs, on this disk.
    pub fn on(fs: ShimFs) -> World {
        let world = World {
            dir: Scratch::new("check"),
            fs: Arc::new(fs),
            photo: picture(96, 64, noisy(11, false)),
            other: picture(96, 64, noisy(12, false)),
            toolbox: Toolbox::standard(),
            grown: Arc::new(AtomicU64::new(0)),
            cached: false,
            recipes: (whizbang_squint::recipes::SHIPPED.iter())
                .map(|(_, text)| (*text).to_owned())
                .collect(),
            more: Vec::new(),
        };
        let store = world.store();
        let mut publication = store.begin().unwrap();
        for image in [&world.photo, &world.other] {
            artifact::put(&mut publication, &Artifact::Image(image.clone())).unwrap();
        }
        publication.publish().unwrap();
        world
    }

    /// Puts these images in the world's store.
    pub fn hold(&self, images: &[&Image]) {
        let store = self.store();
        let mut publication = store.begin().unwrap();
        for image in images {
            artifact::put(&mut publication, &Artifact::Image((*image).clone())).unwrap();
        }
        publication.publish().unwrap();
    }

    pub fn root(&self) -> PathBuf {
        self.dir.path().join("store")
    }

    pub fn store(&self) -> Arc<Store> {
        Arc::new(Store::open(&self.root(), self.fs.clone()).unwrap())
    }

    /// An executor over this world's store whose calls resolve this judge, and that
    /// holds it.
    pub fn engine(&self, judge: Arc<dyn Judge>, secrets: Secrets) -> Engine {
        self.engine_wrapping(judge, secrets, |action| action)
    }

    /// The same executor, with `check` and `view` each as `wrap` gives it back.
    pub fn engine_wrapping(
        &self,
        judge: Arc<dyn Judge>,
        secrets: Secrets,
        wrap: impl Fn(Arc<dyn Action>) -> Arc<dyn Action>,
    ) -> Engine {
        self.engine_priced(judge, secrets, wrap, 0)
    }

    /// The same executor, whose judge states that a call of it may cost `micros`.
    pub fn engine_priced(
        &self,
        judge: Arc<dyn Judge>,
        secrets: Secrets,
        wrap: impl Fn(Arc<dyn Action>) -> Arc<dyn Action>,
        micros: u64,
    ) -> Engine {
        let grow = ActionDescriptor::draft("grow", "1.0.0", Kind::Primitive, "Grows a mask.")
            .input(InputPort::optional("mask", ArtifactType::Image, "A mask."))
            .argument(Argument::optional(
                "px",
                "How far.",
                ValueType::number(None, None),
            ));
        let grow = Grow(grow.build().unwrap(), self.grown.clone());
        let mut registry = Registry::new();
        let check = Check::new(self.toolbox.clone()).unwrap();
        let check = if self.cached { check.caching() } else { check };
        registry.register(wrap(Arc::new(check))).unwrap();
        let view = View::new().unwrap();
        registry.register(wrap(Arc::new(view))).unwrap();
        registry
            .register(wrap(Arc::new(Align::new().unwrap())))
            .unwrap();
        registry
            .register(wrap(Arc::new(Resize::new().unwrap())))
            .unwrap();
        for compare in [Compare::change_map(), Compare::edge_similarity()] {
            registry.register(wrap(Arc::new(compare.unwrap()))).unwrap();
        }
        registry.register(Arc::new(grow)).unwrap();
        // The use cases as recipes (plan m3, task Q1), over the actions above.
        let details = whizbang_squint::measured::DetailsStep::new().unwrap();
        registry.register(wrap(Arc::new(details))).unwrap();
        for action in &self.more {
            registry.register(action.clone()).unwrap();
        }
        for text in &self.recipes {
            let recipe = recipe_of(text, &registry);
            registry.register(Arc::new(recipe)).unwrap();
        }
        let offered = roster(None, None, Some(judge.as_ref()));
        Engine::over(
            registry,
            Arc::new(Priced(offered, micros)),
            self.store(),
            Digest::of_bytes(b""),
        )
        .with_clock(Arc::new(Ticking(AtomicU64::new(1000))))
        .with_capabilities(Capabilities::none().with_judge(judge))
        .with_secrets(secrets)
    }

    /// A use case's recipe (plan m3, task Q1) under a job, of these images, in its order.
    pub fn recipe(job: &whizbang_squint::job::Job, images: &[ArtifactId]) -> Call {
        let mut call = Call::new(&job.use_case)
            .argument("job", whizbang_core::contract::Value::text(&job.to_json()));
        for image in images {
            call = call.input("images", *image);
        }
        call
    }

    /// `check` on the photograph.
    pub fn check(&self) -> Call {
        Call::new("check").input("images", self.photo.id())
    }
}

/// A recipe's text, read and checked against this registry, as an action.
pub fn recipe_of(text: &str, registry: &Registry) -> whizbang_core::recipe::RecipeAction {
    let checked = whizbang_core::recipe::Recipe::read(text)
        .unwrap()
        .check(registry)
        .unwrap();
    whizbang_core::recipe::RecipeAction::new(checked, text, registry).unwrap()
}

/// A scripted judge read from data (`judge-script/1`): the text, with each `$name`
/// replaced by the identity given for it.
pub fn judge(script: &str, names: &[(&str, ArtifactId)]) -> Arc<FakeJudge> {
    let mut script = script.to_owned();
    for (name, id) in names {
        script = script.replace(&format!("${name}"), &id.to_string());
    }
    Arc::new(FakeJudge::from_script(script.as_bytes()).unwrap())
}

/// What a run left in the store, read by a reader that opens the store anew.
pub struct Left {
    pub record: Option<RunRecord>,
    /// Every name of the run, with the kind of its reference and its content.
    pub names: BTreeMap<String, (Stored, Vec<u8>)>,
}

impl Left {
    /// A diagnostic of the run, by the name the action or the executor gave it.
    pub fn diagnostic(&self, name: &str) -> &[u8] {
        let (kind, bytes) = &self.names[&format!("diagnostics/{name}")];
        assert_eq!(*kind, Stored::Diagnostic, "{name}");
        bytes
    }

    pub fn has(&self, name: &str) -> bool {
        self.names.contains_key(&format!("diagnostics/{name}"))
    }

    /// An exchange with the judge, by its number from 1, as JSON.
    pub fn exchange(&self, number: usize) -> Json {
        serde_json::from_slice(self.diagnostic(&format!("exchange-{number:04}"))).unwrap()
    }

    /// The image a piece of evidence is, by its number from 1.
    pub fn evidence(&self, number: usize) -> Image {
        let file = self.diagnostic(&format!("evidence-{number:03}"));
        match Artifact::from_bytes(file, &limits()).unwrap() {
            Artifact::Image(image) => image,
            other => panic!("{:?}", other.artifact_type()),
        }
    }

    pub fn record(&self) -> &RunRecord {
        self.record.as_ref().expect("the run has its record")
    }
}

/// Reads a run from the store at `root`, through a reader of its own on this disk.
/// A record that is there is held to the schema. The run's holds on the artifacts it took
/// (ADR 0008, "What deleting a run removes") are not among its names here: they are
/// references to what it read, not to anything it wrote.
pub fn left(root: &Path, fs: Arc<dyn Fs>, run: &RunId) -> Left {
    let reader = Reader::open(root, fs).unwrap();
    let generation = reader.snapshot().unwrap();
    let entries = generation.run(run.as_str()).unwrap().entries();
    let mut names = BTreeMap::new();
    let written = entries
        .iter()
        .filter(|(name, _)| !name.starts_with(HOLDS_PREFIX));
    for (name, entry) in written {
        names.insert(name.clone(), (entry.kind, reader.read(entry).unwrap()));
    }
    let record = names.get("record").map(|(_, bytes)| {
        let written: Json = serde_json::from_slice(bytes).unwrap();
        // A recipe's run, which lists its steps, is written as run/5 (ADR 0033); a run
        // that handed back findings, as `check` under a use case does, as run/6 (ADR
        // 0005, "run/6").
        let schema = match written["record"].as_str() {
            Some("run/6") => whizbang_core::contract::RUN_6_SCHEMA,
            Some("run/5") => whizbang_core::contract::RUN_5_SCHEMA,
            _ => RUN_RECORD_SCHEMA,
        };
        let schema: Json = serde_json::from_str(schema).unwrap();
        validate(&schema, &written).unwrap();
        RunRecord::from_bytes(bytes).unwrap()
    });
    Left { record, names }
}

/// How many runs the store at `root` holds.
pub fn runs(root: &Path, fs: Arc<dyn Fs>) -> usize {
    let reader = Reader::open(root, fs).unwrap();
    reader.snapshot().unwrap().runs().len()
}

/// Whether the store at `root` holds an image artifact of this identity.
pub fn holds(root: &Path, fs: Arc<dyn Fs>, id: ArtifactId) -> bool {
    let reader = Reader::open(root, fs).unwrap();
    artifact::holds_anywhere(&reader.snapshot().unwrap(), ArtifactType::Image, id)
}

/// Every file under the store, with its bytes.
pub fn files_under(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else {
                files.push((path.clone(), std::fs::read(&path).unwrap()));
            }
        }
    }
    files
}
