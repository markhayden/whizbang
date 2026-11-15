//! What the tests of the spend strand's parts are made of: a store on a scratch
//! folder, a clock a test moves by hand, backends that state a bound, a config's
//! source a test swaps, and actions that do what a test says.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use whizbang_core::contract::record::{self, RunRecord};
use whizbang_core::contract::{
    Action, ActionDescriptor, Answer, Backends, CallError, Capability, Clock, Context, Engine,
    Estimate, Failure, Finished, Kind, ModelIdentity, PreparedCall, Produced, Refusal, RefusalKind,
    Registry, Requirement, RunId, Snapshot, Unavailable,
};
use whizbang_core::identity::Digest;
use whizbang_core::store::{Reader, Store};
use whizbang_testkit::fs_shim::ShimFs;
use whizbang_testkit::scratch::Scratch;
use whizbang_testkit::source::FakeSource;

/// What one request to a model may cost under [`Bounds::quarter`], in millionths.
pub const QUARTER: u64 = 250_000;

/// A clock a test moves by hand.
pub struct Hand(AtomicU64);

impl Hand {
    pub fn at(ms: u64) -> Arc<Hand> {
        Arc::new(Hand(AtomicU64::new(ms)))
    }

    pub fn set(&self, ms: u64) {
        self.0.store(ms, Ordering::SeqCst);
    }
}

impl Clock for Hand {
    fn now_ms(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}

/// Backends with one model for every capability, whose every request may cost this.
pub struct Bounds(pub Estimate);

impl Bounds {
    pub fn quarter() -> Bounds {
        Bounds(Estimate::of_micros(QUARTER))
    }
}

impl Backends for Bounds {
    fn resolve(
        &self,
        _: Capability,
        _: Option<&str>,
        _: &[String],
    ) -> Result<ModelIdentity, Unavailable> {
        Ok(ModelIdentity::new("fake", "fake-model", "1"))
    }

    fn bound(&self, _: Capability, _: &ModelIdentity) -> Estimate {
        self.0
    }
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

/// A store on a scratch folder, a source of the config a test swaps, a clock it moves,
/// and a count of the actions that ran.
pub struct World {
    dir: Scratch,
    pub fs: Arc<ShimFs>,
    pub runs: Arc<AtomicU64>,
    pub source: Arc<FakeSource>,
    pub clock: Arc<Hand>,
    pub store: Arc<Store>,
}

/// What a store holds: its generation's number, each name outside runs, and each run
/// with its names.
pub type Held = (u64, Vec<String>, Vec<(String, Vec<String>)>);

impl World {
    /// A world whose config caps nothing, at 7000 by its clock.
    pub fn new(label: &str) -> World {
        let dir = Scratch::new(label);
        let fs = Arc::new(ShimFs::new());
        let store = Arc::new(Store::open(&dir.path().join("store"), fs.clone()).unwrap());
        World {
            dir,
            fs,
            runs: Arc::new(AtomicU64::new(0)),
            source: Arc::new(FakeSource::of(World::snapshot(None))),
            clock: Hand::at(7000),
            store,
        }
    }

    /// A snapshot of a config that caps one call at this, or at nothing.
    pub fn snapshot(cap: Option<u64>) -> Snapshot {
        let snapshot = Snapshot::of(Digest::of_bytes(b"a config"));
        match cap {
            Some(micros) => snapshot.capped_at(Estimate::of_micros(micros)),
            None => snapshot,
        }
    }

    /// From now on the config caps one call at this, or at nothing.
    pub fn cap(&self, cap: Option<u64>) {
        self.source.swap(World::snapshot(cap));
    }

    fn root(&self) -> PathBuf {
        self.dir.path().join("store")
    }

    /// How many actions ran.
    pub fn ran(&self) -> u64 {
        self.runs.load(Ordering::SeqCst)
    }

    /// An action of this name that needs a model for each of these capabilities,
    /// counts its runs in this world, and does what the test says.
    pub fn action(
        &self,
        name: &str,
        needs: &[Capability],
        body: impl Fn(&PreparedCall, &Context) -> Result<Produced, Failure> + Send + Sync + 'static,
    ) -> Arc<dyn Action> {
        let mut draft = ActionDescriptor::draft(name, "1.0.0", Kind::Primitive, "A test action.");
        for need in needs {
            draft = draft.needs(Requirement::new(*need));
        }
        Arc::new(Scripted {
            descriptor: draft.build().unwrap(),
            runs: self.runs.clone(),
            body: Box::new(body),
        })
    }

    /// An action of this name that needs a filler and does nothing: a call of it may
    /// cost a quarter of a dollar under [`Bounds::quarter`].
    pub fn paints(&self, name: &str) -> Arc<dyn Action> {
        self.action(name, &[Capability::Fill], |_, _| Ok(Produced::nothing()))
    }

    /// An executor over this world's store, source and clock, with these actions and
    /// backends, and no part: a test adds the part it is of.
    pub fn engine(&self, backends: Bounds, actions: Vec<Arc<dyn Action>>) -> Engine {
        let mut registry = Registry::new();
        for action in actions {
            registry.register(action).unwrap();
        }
        let digest = Digest::of_bytes(b"");
        Engine::over(registry, Arc::new(backends), self.store.clone(), digest)
            .with_config(self.source.clone())
            .with_clock(self.clock.clone())
    }

    /// What the store holds, read from its folder, and how many files are under it.
    pub fn held(&self) -> (Held, usize) {
        let reader = Reader::open(&self.root(), self.fs.clone()).unwrap();
        let generation = reader.snapshot().unwrap();
        let runs = generation
            .runs()
            .iter()
            .map(|(id, run)| (id.clone(), run.entries().keys().cloned().collect()));
        let held = (
            generation.number(),
            generation.entries().keys().cloned().collect(),
            runs.collect(),
        );
        let mut files = 0;
        let mut dirs = vec![self.root()];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    dirs.push(path);
                } else {
                    files += 1;
                }
            }
        }
        (held, files)
    }

    /// The record of a run that ended.
    pub fn record(&self, run: &RunId) -> RunRecord {
        let generation = self.store.snapshot().unwrap();
        let stored = record::stored(&generation, run.as_str(), |entry| self.store.read(entry));
        stored.unwrap().unwrap()
    }
}

pub fn ran(answer: Result<Answer, CallError>) -> Finished {
    match answer {
        Ok(Answer::Ran(finished)) => finished,
        other => panic!("the request did not run: {other:?}"),
    }
}

/// The part, the kind and the sentence of a refusal by a part.
pub fn by_part(
    answer: Result<impl std::fmt::Debug, CallError>,
) -> (&'static str, RefusalKind, String) {
    match answer {
        Err(CallError::Refused(Refusal::ByPart {
            part, kind, why, ..
        })) => (part, kind, why),
        other => panic!("not refused by a part: {other:?}"),
    }
}

/// What an action said of the calls it made, for the test to read afterwards.
pub type Said<T> = Arc<Mutex<Vec<T>>>;
