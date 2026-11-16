//! What the tests of the spend strand's parts are made of: a store on a scratch
//! folder, a clock a test moves by hand, backends that state a bound, a config's
//! source a test swaps, and actions that do what a test says.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use whizbang_core::contract::record::{self, RunRecord};
use whizbang_core::contract::{
    Action, ActionDescriptor, Answer, Backends, CallError, Capability, Clock, Closing, Context,
    Declined, Engine, Estimate, Failure, Finished, Kind, Ledger, MadeName, ModelIdentity,
    NotJudged, PreparedCall, Produced, Refusal, RefusalKind, Registry, Requirement, Reread,
    Reservation, RunId, Settled, Snapshot, Step, Unavailable, reservations,
};
use whizbang_core::identity::Digest;
use whizbang_core::spend::fold::{Folded, fold};
use whizbang_core::spend::{Amount, Record, StoreLedger};
use whizbang_core::store::{self, Reader, Store};
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

/// An action of this name that needs a model for each of these capabilities, counts
/// its runs here, and does what the test says.
pub fn counting(
    runs: &Arc<AtomicU64>,
    name: &str,
    needs: &[Capability],
    body: impl Fn(&PreparedCall, &Context) -> Result<Produced, Failure> + Send + Sync + 'static,
) -> Arc<dyn Action> {
    let needs: Vec<_> = needs.iter().map(|need| Requirement::new(*need)).collect();
    stating(runs, name, needs, body)
}

/// The same, with these needs as they are stated.
pub fn stating(
    runs: &Arc<AtomicU64>,
    name: &str,
    needs: Vec<Requirement>,
    body: impl Fn(&PreparedCall, &Context) -> Result<Produced, Failure> + Send + Sync + 'static,
) -> Arc<dyn Action> {
    let mut draft = ActionDescriptor::draft(name, "1.0.0", Kind::Primitive, "A test action.");
    for need in needs {
        draft = draft.needs(need);
    }
    Arc::new(Scripted {
        descriptor: draft.build().unwrap(),
        runs: runs.clone(),
        body: Box::new(body),
    })
}

/// Any backends, answering as the bound of one request that of this many: what stands
/// behind an action of a test whose one need is for that many requests, where the test
/// is of what a run does with what it reserved and its action is made elsewhere. What
/// makes the bound of a need cover the requests the need states is preparing a call,
/// and its tests are in `quote.rs`.
pub struct Times<B: ?Sized>(pub u64, pub Arc<B>);

impl<B: Backends + ?Sized> Backends for Times<B> {
    fn resolve(
        &self,
        capability: Capability,
        choice: Option<&str>,
        features: &[String],
    ) -> Result<ModelIdentity, Unavailable> {
        self.1.resolve(capability, choice, features)
    }

    fn bound(&self, capability: Capability, model: &ModelIdentity) -> Estimate {
        let one = self.1.bound(capability, model);
        Estimate::of_micros(one.micros().saturating_mul(self.0))
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
        counting(&self.runs, name, needs, body)
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

// --- the real ledger, and the books of a store -------------------------------------------------

/// The real ledger, with a count of how many times the executor asked it for each of
/// the three things it asks. It judges nothing itself.
#[derive(Debug, Default)]
pub struct Counted {
    inner: StoreLedger,
    asked: AtomicU64,
    confirmed: AtomicU64,
    settled: AtomicU64,
    left_open: bool,
}

impl Counted {
    pub fn new() -> Arc<Counted> {
        Arc::new(Counted::default())
    }

    /// The same, but that it closes nothing when a run ends: for a test of what is
    /// judged against a reservation that still counts. A run of such a test does
    /// nothing and ends at once, and its end closes its reservation and releases all
    /// of it (plan 1B, task S11; `spend::close::`). Left open, the books are those of
    /// a run that is still executing, or of one whose process died before its end.
    /// It reserves and confirms as the real ledger does, and is still counted as
    /// asked to close.
    pub fn left_open() -> Arc<Counted> {
        Arc::new(Counted {
            left_open: true,
            ..Counted::default()
        })
    }

    /// How many times admission asked for a reservation: once for every attempt at
    /// an admission's publication, those the store refused and those the ledger
    /// declined among them.
    pub fn asked(&self) -> u64 {
        self.asked.load(Ordering::SeqCst)
    }

    /// How many admissions it was asked to confirm.
    pub fn confirmed(&self) -> u64 {
        self.confirmed.load(Ordering::SeqCst)
    }

    /// How many times it was asked to close a reservation at the end of a run.
    pub fn settled(&self) -> u64 {
        self.settled.load(Ordering::SeqCst)
    }
}

impl Ledger for Counted {
    fn reserve(
        &self,
        reservation: &Reservation,
        step: &mut Step<'_, '_>,
    ) -> Result<MadeName, Declined> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        self.inner.reserve(reservation, step)
    }

    fn confirm(&self, reservation: &Reservation, reading: &Reread<'_>) -> Result<bool, NotJudged> {
        self.confirmed.fetch_add(1, Ordering::SeqCst);
        self.inner.confirm(reservation, reading)
    }

    fn settle(&self, closing: &Closing<'_>, step: &mut Step<'_, '_>) -> Result<Settled, NotJudged> {
        self.settled.fetch_add(1, Ordering::SeqCst);
        if self.left_open {
            return Ok(Settled::Nothing);
        }
        self.inner.settle(closing, step)
    }
}

/// Other hands, or a command there is not yet, make these records in one publication,
/// each under its own name.
pub fn plant(store: &Store, records: &[Record]) {
    let named: Vec<(String, Vec<u8>)> = records
        .iter()
        .map(|record| (record.name().to_string(), record.to_bytes()))
        .collect();
    plant_bytes(store, &named);
}

/// The same, of names with any content: what is damaged, or of no form.
pub fn plant_bytes(store: &Store, named: &[(String, Vec<u8>)]) {
    let mut publication = store.begin().unwrap();
    for (name, bytes) in named {
        publication
            .create(name, store::Kind::Diagnostic, bytes)
            .unwrap();
    }
    publication.publish().unwrap();
}

/// The books of the store's current generation, by the real fold.
pub fn books(store: &Store) -> Folded {
    fold(&store.snapshot().unwrap(), |entry| store.read(entry))
}

/// The owner's reset, as `whizbang spend reset` makes it (`spend::reset`; plan 1B,
/// task S9): the next epoch with this allowance, its guard, and the next turn of the
/// epoch it closes, in one publication. What comes back is the epoch it opened.
pub fn allow(store: &Store, micros: u64) -> u64 {
    let allowance = Amount::new(micros).unwrap();
    whizbang_core::spend::reset(store, allowance, 0)
        .unwrap()
        .epoch
}

/// Every name the store holds under `spend/`, in their order.
pub fn spend_names(store: &Store) -> Vec<String> {
    let generation = store.snapshot().unwrap();
    let names = generation.entries().keys();
    names
        .filter(|name| name.starts_with("spend/"))
        .cloned()
        .collect()
}

/// Every reservation the store's current generation names.
pub fn reserved(store: &Store) -> Vec<Reservation> {
    let generation = store.snapshot().unwrap();
    reservations(&generation, |entry| store.read(entry)).unwrap()
}

/// The executor's record of a reservation of this run, as it is stored: what other
/// hands leave beside a hold to make a reservation the ledger could have written.
pub fn reservation_bytes(run: &str, micros: u64) -> (String, Vec<u8>) {
    let bytes = format!(
        "{{\"record\":\"reservation/1\",\"run\":\"{run}\",\"request\":null,\
         \"admission\":\"sha256:{}\",\"reserved_micros\":{micros},\"start_ms\":0}}\n",
        "0".repeat(64)
    );
    (format!("spend/reservations/{run}"), bytes.into_bytes())
}
