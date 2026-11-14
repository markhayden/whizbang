//! What the tests of `check` run in: a store on the test kit's disk, an executor that
//! holds `check`, `view`, a stand-in for rascal's `grow`, and a scripted judge read
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
    Action, ActionDescriptor, Argument, Call, Capabilities, Clock, Context, Engine, Failure,
    InputPort, Kind, PreparedCall, Produced, RUN_RECORD_SCHEMA, Registry, RunId, Secrets,
    ValueType,
};
use whizbang_core::identity::Digest;
use whizbang_core::store::fs::Fs;
use whizbang_core::store::{Kind as Stored, Reader, Store};
use whizbang_squint::check::{Check, Toolbox};
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
        };
        let store = world.store();
        let mut publication = store.begin().unwrap();
        for image in [&world.photo, &world.other] {
            artifact::put(&mut publication, &Artifact::Image(image.clone())).unwrap();
        }
        publication.publish().unwrap();
        world
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
        registry.register(wrap(Arc::new(check))).unwrap();
        let view = View::new().unwrap();
        registry.register(wrap(Arc::new(view))).unwrap();
        registry.register(Arc::new(grow)).unwrap();
        let offered = roster(None, None, Some(judge.as_ref()));
        Engine::over(
            registry,
            Arc::new(offered),
            self.store(),
            Digest::of_bytes(b""),
        )
        .with_clock(Arc::new(Ticking(AtomicU64::new(1000))))
        .with_capabilities(Capabilities::none().with_judge(judge))
        .with_secrets(secrets)
    }

    /// `check` on the photograph.
    pub fn check(&self) -> Call {
        Call::new("check").input("images", self.photo.id())
    }
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
/// A record that is there is held to the schema.
pub fn left(root: &Path, fs: Arc<dyn Fs>, run: &RunId) -> Left {
    let reader = Reader::open(root, fs).unwrap();
    let generation = reader.snapshot().unwrap();
    let entries = generation.run(run.as_str()).unwrap().entries();
    let mut names = BTreeMap::new();
    for (name, entry) in entries {
        names.insert(name.clone(), (entry.kind, reader.read(entry).unwrap()));
    }
    let record = names.get("record").map(|(_, bytes)| {
        let schema: Json = serde_json::from_str(RUN_RECORD_SCHEMA).unwrap();
        validate(&schema, &serde_json::from_slice(bytes).unwrap()).unwrap();
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
    artifact::holds(&reader.snapshot().unwrap(), ArtifactType::Image, id)
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
