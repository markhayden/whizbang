//! The commit step (ADR 0008): an action hands back artifacts, and the executor
//! publishes all of them with the run's completed status in the one publication that
//! ends the run, and reports the completed outcome only after that publication is
//! durable. And a panic in an action, which ends its run through the same publication.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use whizbang_core::artifact::{self, Artifact, ArtifactType, Mask, Source, UseCase};
use whizbang_core::contract::record::{self, RunRecord, Status};
use whizbang_core::contract::{
    ADMITTED_NAME, Action, ActionDescriptor, Answer, Argument, Call, CallError, Clock, Committed,
    Context, Count, Engine, Executor, Failure, InputPort, Kind, NoBackends, Outcome, OutputPort,
    PreparedCall, Produced, Registry, Request, RequestId, RunState, Secrets, Value, ValueType,
    When,
};
use whizbang_core::identity::Digest;
use whizbang_core::store::fs::Fs;
use whizbang_core::store::{self, Reader, Store};
use whizbang_testkit::fs_shim::{Event, Lost, ShimFs};
use whizbang_testkit::schema::validate;
use whizbang_testkit::scratch::Scratch;

use super::record::schema;
use super::redact::made_up_key;
use crate::artifact::support::{limits, line, made_by, on, rect, rect_mask};

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

/// The action `pair`, which returns a mask at `mask` and a location at `where`, and,
/// for a call that gives `label`, a second location at `extra`. What it hands back is
/// whatever the test says.
fn pair(
    body: impl Fn(&PreparedCall, &Context) -> Result<Produced, Failure> + Send + Sync + 'static,
) -> Arc<dyn Action> {
    let draft = ActionDescriptor::draft("pair", "1.0.0", Kind::Primitive, "Returns two things.")
        .argument(Argument::optional(
            "label",
            "Return a third thing too.",
            ValueType::Text,
        ))
        .output(OutputPort::one("mask", ArtifactType::Mask, "A mask."))
        .output(OutputPort::one(
            "where",
            ArtifactType::Location,
            "A location.",
        ))
        .output(
            OutputPort::counted(
                "extra",
                ArtifactType::Location,
                "Another location.",
                Count::between(1, 1),
            )
            .when(When::given("label")),
        );
    Arc::new(Scripted(draft.build().unwrap(), Box::new(body)))
}

/// The mask and the location `pair` returns when it returns what it should.
fn the_mask() -> Mask {
    rect_mask(on("photo"))
}

fn the_place() -> whizbang_core::artifact::Location {
    line(on("photo"))
}

fn both() -> Produced {
    Produced::nothing()
        .with("mask", the_mask())
        .with("where", the_place())
}

struct World {
    dir: Scratch,
    fs: Arc<ShimFs>,
}

impl World {
    fn new() -> World {
        World::on(ShimFs::new())
    }

    fn on(fs: ShimFs) -> World {
        World {
            dir: Scratch::new("commit"),
            fs: Arc::new(fs),
        }
    }

    fn root(&self) -> PathBuf {
        self.dir.path().join("store")
    }

    fn store(&self) -> Arc<Store> {
        Arc::new(Store::open(&self.root(), self.fs.clone()).unwrap())
    }

    fn engine(&self, actions: Vec<Arc<dyn Action>>) -> Engine {
        engine_over(self.store(), actions)
    }
}

fn engine_over(store: Arc<Store>, actions: Vec<Arc<dyn Action>>) -> Engine {
    let mut registry = Registry::new();
    for action in actions {
        registry.register(action).unwrap();
    }
    Engine::over(registry, Arc::new(NoBackends), store, Digest::of_bytes(b""))
        .with_clock(Arc::new(Stopped(5000)))
}

/// What a store holds of the runs of a test, read by a reader of its own: for each
/// run, its record when it has one, and whether the run holds anything but its
/// admission; and whether the store holds the mask and the location, each read whole
/// and checked.
struct Held {
    runs: Vec<(String, Option<RunRecord>, usize)>,
    /// The names of all the runs that are references to a committed output, each with
    /// the content read through it as an output.
    outputs: Vec<(String, Vec<u8>)>,
    mask: bool,
    place: bool,
    generation: u64,
}

fn held(root: &std::path::Path, fs: Arc<dyn Fs>) -> Held {
    let reader = Reader::open(root, fs).unwrap();
    let generation = reader.snapshot().unwrap();
    let runs = generation
        .runs()
        .iter()
        .map(|(id, run)| {
            assert!(run.get(ADMITTED_NAME).is_some(), "{id} has no admission");
            let stored = record::stored(&generation, id, |entry| reader.read(entry));
            let stored = match stored.unwrap() {
                Ok(record) => Some(record),
                Err(record::StoredRecordError::NoRecord) => None,
                Err(other) => panic!("{other}"),
            };
            (id.clone(), stored, run.entries().len())
        })
        .collect();
    let outputs = generation
        .runs()
        .values()
        .flat_map(|run| run.entries())
        .filter(|(_, entry)| entry.kind == store::Kind::Output)
        .map(|(name, entry)| (name.clone(), reader.read_output(entry).unwrap()))
        .collect();
    let has = |kind, id| match artifact::get(&reader, kind, id, &limits()) {
        Ok(_) => true,
        Err(artifact::ArtifactError::Missing { .. }) => false,
        Err(other) => panic!("{other}"),
    };
    Held {
        runs,
        outputs,
        mask: has(ArtifactType::Mask, the_mask().id()),
        place: has(ArtifactType::Location, the_place().id()),
        generation: generation.number(),
    }
}

fn committed() -> Vec<Committed> {
    vec![
        Committed::new("mask", ArtifactType::Mask, the_mask().id()),
        Committed::new("where", ArtifactType::Location, the_place().id()),
    ]
}

// --- the commit -------------------------------------------------------------------------------

#[test]
fn an_action_hands_back_artifacts_and_they_are_committed_with_the_completed_status() {
    let world = World::new();
    // Handed back in the other order: the outcome and the record list them in the
    // order of the descriptor's ports.
    let engine = world.engine(vec![pair(|_, _| {
        Ok(Produced::nothing()
            .with("where", the_place())
            .with("mask", the_mask()))
    })]);
    let before = held(&world.root(), world.fs.clone());
    assert!(!before.mask && !before.place);

    let request = Request::new(Call::new("pair")).with_id(RequestId::parse("r-1").unwrap());
    let Ok(Answer::Ran(finished)) = engine.request(&request) else {
        panic!("the request did not run");
    };
    assert_eq!(
        finished.outcome(),
        &Outcome::Completed {
            outputs: committed()
        }
    );

    // One publication for the admission and one for the end: the outputs and the
    // completed status arrived in the second, together.
    let after = held(&world.root(), world.fs.clone());
    assert_eq!(after.generation, 2);
    assert!(after.mask && after.place);
    let (run, stored, _) = &after.runs[0];
    assert_eq!(run, finished.run().as_str());
    let stored = stored.as_ref().unwrap();
    assert_eq!(stored.outcome().status, Status::Completed);
    let listed: Vec<_> = stored
        .outputs()
        .iter()
        .map(|o| (o.port.as_str(), o.artifact_type, o.artifact))
        .collect();
    assert_eq!(
        listed,
        [
            ("mask", ArtifactType::Mask, the_mask().id().0),
            ("where", ArtifactType::Location, the_place().id().0),
        ]
    );
    let document: serde_json::Value = serde_json::from_slice(&stored.to_bytes()).unwrap();
    validate(&schema(), &document).unwrap();

    // Each is stored as its canonical file, under the name of its type and identity,
    // as a committed output: the bytes any other store would hold.
    let reader = Reader::open(&world.root(), world.fs.clone()).unwrap();
    let generation = reader.snapshot().unwrap();
    let name = artifact::name(ArtifactType::Mask, the_mask().id());
    let entry = generation.get(&name).unwrap();
    assert_eq!(entry.kind, store::Kind::Output);
    assert_eq!(
        reader.read_output(entry).unwrap(),
        Artifact::from(the_mask()).to_bytes()
    );

    // And the run holds a reference of its own to each, as a committed output, under
    // the port it was returned at and its place there: the same file.
    assert_eq!(
        after.outputs,
        [
            (
                "outputs/mask/0".to_owned(),
                Artifact::from(the_mask()).to_bytes()
            ),
            (
                "outputs/where/0".to_owned(),
                Artifact::from(the_place()).to_bytes()
            ),
        ]
    );

    // The same request again is answered with what the first answer said.
    let Ok(Answer::Repeat(earlier)) = engine.request(&request) else {
        panic!("not answered as a repeat");
    };
    assert_eq!(
        earlier.state(),
        &RunState::Ended(finished.outcome().clone())
    );
}

#[test]
fn a_port_that_returns_several_numbers_the_references_of_its_run() {
    let several = {
        let draft = ActionDescriptor::draft("several", "1.0.0", Kind::Primitive, "Returns some.")
            .output(OutputPort::counted(
                "found",
                ArtifactType::Location,
                "What was found.",
                Count::between(0, 3),
            ));
        Arc::new(Scripted(
            draft.build().unwrap(),
            Box::new(|_, _| {
                Ok(Produced::nothing()
                    .with("found", the_place())
                    .with("found", the_place()))
            }),
        ))
    };
    let world = World::new();
    let engine = world.engine(vec![several]);
    let finished = engine.submit(&Call::new("several")).unwrap();
    // The same artifact twice is two outputs of the run, and one artifact in the store.
    assert_eq!(finished.outcome().outputs().len(), 2);
    let after = held(&world.root(), world.fs.clone());
    let names: Vec<&str> = after
        .outputs
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(names, ["outputs/found/0", "outputs/found/1"]);
    assert!(after.place);
}

#[test]
fn deleting_a_run_removes_its_references_and_known_leaves_the_artifacts_it_committed() {
    // The limit, pinned: an artifact is stored outside runs, under the name a later
    // call takes it by, and deleting the run that committed it removes the run's own
    // references and not that name (`docs/carry-over.md`).
    let world = World::new();
    let store = world.store();
    let engine = engine_over(store.clone(), vec![pair(|_, _| Ok(both()))]);
    let finished = engine.submit(&Call::new("pair")).unwrap();
    store.delete(&[finished.run().as_str()]).unwrap();
    store.prune().unwrap();
    let after = held(&world.root(), world.fs.clone());
    assert!(after.runs.is_empty() && after.outputs.is_empty());
    assert!(after.mask && after.place);
}

#[test]
fn an_output_one_run_committed_is_an_input_of_the_next() {
    let world = World::new();
    let look = {
        let draft = ActionDescriptor::draft("look", "1.0.0", Kind::Primitive, "Looks.")
            .input(InputPort::required("mask", ArtifactType::Mask, "The mask."));
        Arc::new(Scripted(
            draft.build().unwrap(),
            Box::new(|_, _| Ok(Produced::nothing())),
        ))
    };
    let engine = world.engine(vec![pair(|_, _| Ok(both())), look]);
    let call = Call::new("look").input("mask", the_mask().id());
    // Before the run that makes it, the mask is not there to be taken.
    assert!(matches!(engine.submit(&call), Err(CallError::Refused(_))));
    engine.submit(&Call::new("pair")).unwrap();
    let finished = engine.submit(&call).unwrap();
    let after = held(&world.root(), world.fs.clone());
    let stored = after
        .runs
        .iter()
        .find(|(run, _, _)| run == finished.run().as_str());
    let stored = stored.unwrap().1.as_ref().unwrap();
    assert_eq!(stored.inputs()[0].artifact, the_mask().id().0);
}

#[test]
fn what_an_action_hands_back_prints_as_shapes_and_never_as_content() {
    let produced = both();
    let printed = format!("{produced:?}");
    assert_eq!(
        printed,
        format!(
            "[mask at \"mask\": {}, location at \"where\": {}]",
            the_mask().id(),
            the_place().id()
        )
    );
    assert_eq!(format!("{:?}", Produced::nothing()), "[]");
    let ports: Vec<&str> = produced.outputs().map(|(port, _)| port).collect();
    assert_eq!(ports, ["mask", "where"]);
}

// --- what is handed back is held to the ports the call returns --------------------------------

#[test]
fn what_an_action_hands_back_is_held_to_the_ports_its_call_returns() {
    // Each of these is an action that finished and handed back what its descriptor
    // does not say: the run ends as an error of the type `output`, and nothing of what
    // was handed back is committed, the artifacts that were right included.
    type Case = (&'static str, bool, fn() -> Produced, &'static str);
    let cases: [Case; 6] = [
        (
            "a port that is not declared",
            false,
            || both().with("nope", the_place()),
            "the action handed back an artifact at \"nope\", which is no output port this \
             call returns",
        ),
        (
            "an artifact of another type",
            false,
            || {
                Produced::nothing()
                    .with("mask", the_place())
                    .with("where", the_place())
            },
            "the action handed back a location at the output port \"mask\", which returns \
             a mask",
        ),
        (
            "more than the port returns",
            false,
            || both().with("mask", the_mask()),
            "the action handed back 2 artifacts at the output port \"mask\", which returns \
             1 at most",
        ),
        (
            "fewer than the port returns",
            false,
            || Produced::nothing().with("mask", the_mask()),
            "the action handed back 0 artifacts at the output port \"where\", which \
             returns 1 at least",
        ),
        (
            "a port this call does not return",
            false,
            || both().with("extra", the_place()),
            "the action handed back an artifact at \"extra\", which is no output port this \
             call returns",
        ),
        (
            "a port this call returns, left out",
            true,
            both,
            "the action handed back 0 artifacts at the output port \"extra\", which \
             returns 1 at least",
        ),
    ];
    for (case, labelled, hand_back, said) in cases {
        let world = World::new();
        let engine = world.engine(vec![pair(move |_, _| Ok(hand_back()))]);
        let call = if labelled {
            Call::new("pair").argument("label", Value::text("x"))
        } else {
            Call::new("pair")
        };
        let finished = engine.submit(&call).unwrap();
        assert_eq!(
            finished.outcome(),
            &Outcome::Error {
                kind: "output".to_owned(),
                message: said.to_owned(),
            },
            "{case}"
        );
        let after = held(&world.root(), world.fs.clone());
        assert!(!after.mask && !after.place, "{case}");
        assert!(after.outputs.is_empty(), "{case}");
        let stored = after.runs[0].1.as_ref().unwrap();
        assert_eq!(stored.outcome().status, Status::Error, "{case}");
        assert!(stored.outputs().is_empty(), "{case}");
    }

    // The port a call returns only when it gives an argument is returned when it does.
    let world = World::new();
    let engine = world.engine(vec![pair(|_, _| Ok(both().with("extra", the_place())))]);
    let call = Call::new("pair").argument("label", Value::text("x"));
    let finished = engine.submit(&call).unwrap();
    let mut outputs = committed();
    outputs.push(Committed::new(
        "extra",
        ArtifactType::Location,
        the_place().id(),
    ));
    assert_eq!(finished.outcome(), &Outcome::Completed { outputs });
}

#[test]
fn an_action_that_fails_commits_nothing() {
    for failure in [Failure::error("pair", "no"), Failure::invalid("no")] {
        let world = World::new();
        let engine = world.engine(vec![pair(move |_, _| Err(failure.clone()))]);
        let finished = engine.submit(&Call::new("pair")).unwrap();
        assert!(finished.outcome().outputs().is_empty());
        let after = held(&world.root(), world.fs.clone());
        assert!(!after.mask && !after.place);
        assert!(after.runs[0].1.as_ref().unwrap().outputs().is_empty());
    }
}

// --- an artifact the store already holds ------------------------------------------------------

/// The mask `pair` returns, as another action would have made it: the same planes on
/// the same image, so the same identity, and another origin, so another file.
fn the_mask_made_otherwise() -> Mask {
    Mask::from_core(
        on("photo"),
        UseCase::Custom,
        rect(2, 1, 5, 4),
        Source::default(),
        made_by("another"),
    )
    .unwrap()
}

#[test]
fn an_output_the_store_already_holds_keeps_the_file_it_has() {
    let first = Artifact::from(the_mask_made_otherwise());
    assert_eq!(first.id(), the_mask().id());
    assert_ne!(first.to_bytes(), Artifact::from(the_mask()).to_bytes());
    let stored_mask = |world: &World| {
        let reader = Reader::open(&world.root(), world.fs.clone()).unwrap();
        let generation = reader.snapshot().unwrap();
        let name = artifact::name(ArtifactType::Mask, the_mask().id());
        reader.read_output(generation.get(&name).unwrap()).unwrap()
    };

    // Held before the run started.
    let world = World::new();
    let store = world.store();
    let mut publication = store.begin().unwrap();
    artifact::put(&mut publication, &first).unwrap();
    publication.publish().unwrap();
    let engine = engine_over(store, vec![pair(|_, _| Ok(both()))]);
    let finished = engine.submit(&Call::new("pair")).unwrap();
    assert_eq!(
        finished.outcome(),
        &Outcome::Completed {
            outputs: committed()
        }
    );
    assert_eq!(stored_mask(&world), first.to_bytes());
    let after = held(&world.root(), world.fs.clone());
    assert!(after.place);
    // The run's own reference is to the file this run handed back, with its origin.
    assert_eq!(
        after.outputs[0],
        (
            "outputs/mask/0".to_owned(),
            Artifact::from(the_mask()).to_bytes()
        )
    );

    // Published by someone else while the commit was on its way: after the run's
    // outputs were moved into the store, and before its generation was written. The
    // store decides inside its publishing step, so the first one published is kept.
    let world = World::new();
    let store = world.store();
    let engine = engine_over(store.clone(), vec![pair(|_, _| Ok(both()))]);
    let start = world.fs.events().len();
    let moved = {
        // The same run, somewhere else, to learn which call syncs the stored content
        // of the commit: the second sync of `objects/` after the store was opened.
        let other = World::new();
        let engine = other.engine(vec![pair(|_, _| Ok(both()))]);
        let opened = other.fs.events().len();
        assert_eq!(opened, start);
        engine.submit(&Call::new("pair")).unwrap();
        let synced =
            |event: &Event| matches!(event, Event::SyncDir(dir) if dir.ends_with("objects"));
        let events = other.fs.events();
        let syncs: Vec<usize> = (opened..events.len())
            .filter(|at| synced(&events[*at]))
            .collect();
        assert_eq!(syncs.len(), 2, "{events:?}");
        syncs[1]
    };
    let published = Arc::new(AtomicUsize::new(0));
    // Paused just before that sync: the outputs are in `objects/`, and the step that
    // one publication runs at a time is not yet taken.
    world.fs.pause_before(moved, {
        let (store, first, published) = (store.clone(), first.clone(), published.clone());
        move || {
            let mut publication = store.begin().unwrap();
            artifact::put(&mut publication, &first).unwrap();
            publication.publish().unwrap();
            published.fetch_add(1, Ordering::SeqCst);
        }
    });
    let finished = engine.submit(&Call::new("pair")).unwrap();
    assert_eq!(published.load(Ordering::SeqCst), 1);
    assert_eq!(finished.outcome().outputs(), committed());
    assert_eq!(stored_mask(&world), first.to_bytes());
    let after = held(&world.root(), world.fs.clone());
    assert!(after.mask && after.place);
    assert_eq!(after.generation, 3);
}

// --- none or all ------------------------------------------------------------------------------

/// The calls that change the disk from the moment `pair` hands back its two artifacts
/// to the moment its run is reported: where the first is among all the calls made on a
/// new store, and how many there are.
fn the_calls_of_a_commit() -> (usize, usize) {
    let world = World::new();
    let started = Arc::new(AtomicU64::new(0));
    let noting = {
        let (fs, started) = (world.fs.clone(), started.clone());
        pair(move |_, _| {
            started.store(fs.events().len() as u64, Ordering::SeqCst);
            Ok(both())
        })
    };
    let engine = world.engine(vec![noting]);
    engine.submit(&Call::new("pair")).unwrap();
    let start = started.load(Ordering::SeqCst) as usize;
    (start, world.fs.events().len() - start)
}

#[test]
fn a_run_crashed_inside_its_commit_reopens_with_neither_output_nor_status_or_with_all() {
    // The action handed back a mask and a location. Power is lost before each call
    // that changes the disk from then on, with each way a disk can lose what was not
    // yet durable: every name undone, every name kept, and every choice of which
    // directories, and of which names, lose theirs. A wrong commit would write the two
    // outputs, or the outputs and the status, in steps that a crash can separate.
    let (start, steps) = the_calls_of_a_commit();
    assert!(steps > 10, "{steps}");
    let mut outcomes = BTreeSet::new();
    let (mut crashes, mut reported, mut most) = (0, 0, 0);
    for step in 0..steps {
        let mut ways = vec![Lost::Unsynced, Lost::ContentOnly, Lost::Subset(0)];
        let mut next = 0;
        while next < ways.len() {
            let lost = ways[next];
            next += 1;
            crashes += 1;
            let world = World::on(ShimFs::new().crash_before(start + step, lost));
            let engine = world.engine(vec![pair(|_, _| Ok(both()))]);
            let answer = engine.submit(&Call::new("pair"));
            assert!(world.fs.crashed(), "step {step}");
            if lost == Lost::Subset(0) {
                // Every choice of directories. Of names: every choice of the names
                // outside `staging/`, each with the names inside it all lost and all
                // kept. The next writer removes whatever is in staging before it does
                // anything else, so which of those names a crash kept should decide
                // nothing of what it then reads: the two extremes are tried, and not
                // each name apart, which is two thousand crashes at one step, where
                // eleven names are undecided.
                let (dirs, named) = (world.fs.crash_choices(), world.fs.crash_named());
                let staging = world.root().join("staging");
                let bit = |names: &[usize]| names.iter().map(|at| 1u64 << at).sum::<u64>();
                let (inside, outside): (Vec<usize>, Vec<usize>) =
                    (0..named.len()).partition(|at| named[*at].starts_with(&staging));
                assert!(dirs <= 6 && outside.len() <= 7, "step {step}: {named:?}");
                most = most.max(outside.len());
                ways.extend((1..1u64 << dirs).map(Lost::Subset));
                for choice in 0..1u64 << outside.len() {
                    let chosen: Vec<usize> = (0..outside.len())
                        .filter(|at| choice >> at & 1 == 1)
                        .map(|at| outside[at])
                        .collect();
                    ways.push(Lost::Names(bit(&chosen)));
                    if !inside.is_empty() {
                        ways.push(Lost::Names(bit(&chosen) | bit(&inside)));
                    }
                }
            }
            drop(engine);

            // The next writer recovers the store, and a reader reads what survived.
            let fs: Arc<dyn Fs> = Arc::new(ShimFs::new());
            drop(Store::open(&world.root(), fs.clone()).unwrap());
            let after = held(&world.root(), fs);
            let at = format!("step {step}, {lost:?}");
            assert_eq!(after.runs.len(), 1, "{at}");
            let (_, stored, names) = &after.runs[0];
            match stored {
                None => {
                    // Neither output, and no status: the run is as it was admitted.
                    assert!(!after.mask && !after.place, "{at}");
                    assert_eq!(*names, 1, "{at}");
                    assert!(after.outputs.is_empty(), "{at}");
                    assert!(
                        matches!(answer, Err(CallError::Store(_))),
                        "{at}: a run was reported whose commit did not survive: {answer:?}"
                    );
                    outcomes.insert("neither");
                }
                Some(stored) => {
                    // Both outputs, each whole, and the completed status that lists
                    // them.
                    assert!(after.mask && after.place, "{at}");
                    assert_eq!(stored.outcome().status, Status::Completed, "{at}");
                    assert_eq!(stored.outputs().len(), 2, "{at}");
                    assert_eq!(after.outputs.len(), 2, "{at}");
                    assert_eq!(*names, 5, "{at}");
                    outcomes.insert("all");
                    // Reported only where the last call left was the clearing of the
                    // staging, which nothing depends on.
                    if let Ok(finished) = &answer {
                        assert_eq!(finished.outcome().outputs(), committed(), "{at}");
                        assert_eq!(step, steps - 1, "{at}");
                        reported += 1;
                    }
                }
            }
        }
    }
    assert_eq!(outcomes.into_iter().collect::<Vec<_>>(), ["all", "neither"]);
    // The files of the two outputs, of the journal and of the record were undecided
    // together, each apart from the others, at one step at least.
    assert_eq!(most, 4);
    assert!(crashes > 10 * steps, "{crashes} crashes over {steps} steps");
    assert!(reported > 0);
}

#[test]
fn a_commit_that_the_store_refuses_at_any_step_publishes_no_output_and_no_status() {
    // The store fails at each call of the commit in turn, without a crash. The answer
    // is the store's failure; what a reader then sees is the run as it was admitted and
    // neither output, or, where the commit became visible and its last sync failed,
    // all of it.
    let (start, steps) = the_calls_of_a_commit();
    let mut seen = BTreeSet::new();
    for step in 0..steps {
        let world = World::new();
        let engine = world.engine(vec![pair(|_, _| Ok(both()))]);
        world.fs.refuse_before(start + step);
        let answer = engine.submit(&Call::new("pair"));
        let after = held(&world.root(), world.fs.clone());
        let (_, stored, names) = &after.runs[0];
        match answer {
            Err(CallError::Store(failure)) => {
                let said = CallError::Store(failure).to_string();
                assert!(!said.contains(world.root().to_str().unwrap()), "{said}");
                let unconfirmed = said.contains("not confirmed durable");
                assert_eq!(unconfirmed, stored.is_some(), "step {step}: {said}");
                assert_eq!(after.mask, unconfirmed, "step {step}");
                assert_eq!(after.place, unconfirmed, "step {step}");
                assert_eq!(*names == 1, !unconfirmed, "step {step}");
                assert_eq!(after.outputs.len(), if unconfirmed { 2 } else { 0 });
                seen.insert(unconfirmed);
            }
            // Clearing the staging away, after the commit is durable, can fail without
            // failing the run.
            Ok(finished) => {
                assert_eq!(step, steps - 1);
                assert_eq!(finished.outcome().outputs(), committed());
                assert!(after.mask && after.place && stored.is_some());
            }
            other => panic!("step {step}: {other:?}"),
        }
    }
    assert_eq!(seen.into_iter().collect::<Vec<_>>(), [false, true]);
}

#[test]
fn a_completed_run_that_was_reported_has_its_outputs_after_a_loss_of_power() {
    let world = World::new();
    let engine = world.engine(vec![pair(|_, _| Ok(both()))]);
    let finished = engine.submit(&Call::new("pair")).unwrap();
    assert_eq!(finished.outcome().outputs(), committed());
    // Power is lost the moment the run was reported: everything not yet durable goes.
    world.fs.crash(Lost::Unsynced);
    drop(engine);
    let fs: Arc<dyn Fs> = Arc::new(ShimFs::new());
    let after = held(&world.root(), fs);
    assert!(after.mask && after.place);
    let stored = after.runs[0].1.as_ref().unwrap();
    assert_eq!(stored.outcome().status, Status::Completed);
    assert_eq!(stored.outputs().len(), 2);
}

// --- a panic ----------------------------------------------------------------------------------

#[test]
fn a_panic_in_an_action_ends_its_run_as_an_error_with_its_record() {
    let key = made_up_key("commit");
    let world = World::new();
    let panicking = {
        let key = key.clone();
        pair(move |_, context| {
            context.events().progress("about to go wrong");
            context
                .diagnostics()
                .write("seen.txt", b"evidence")
                .unwrap();
            panic!("the plane was {key} and not a plane");
        })
    };
    let engine = world
        .engine(vec![panicking])
        .with_secrets(Secrets::new([key.clone()]).unwrap());
    let request = Request::new(Call::new("pair")).with_id(RequestId::parse("r-1").unwrap());
    let Ok(Answer::Ran(finished)) = engine.request(&request) else {
        panic!("the request did not run");
    };
    let outcome = Outcome::Error {
        kind: "panic".to_owned(),
        message: "the action panicked: the plane was [redacted] and not a plane".to_owned(),
    };
    assert_eq!(finished.outcome(), &outcome);

    // The run ended, through the same publication as any other: its record, its
    // journal and what it wrote before it went wrong are there, and no output.
    let after = held(&world.root(), world.fs.clone());
    assert_eq!(after.generation, 2);
    let (_, stored, names) = &after.runs[0];
    let stored = stored.as_ref().unwrap();
    assert_eq!(stored.outcome().status, Status::Error);
    assert_eq!(stored.outcome().error.as_ref().unwrap().kind, "panic");
    let diagnostics: Vec<&str> = stored.diagnostics().iter().map(|d| d.name()).collect();
    assert_eq!(diagnostics, ["diagnostics/seen.txt", "journal"]);
    assert_eq!(*names, 4);
    assert!(!after.mask && !after.place);

    // So the same request again is told how the run ended, and not that it is still
    // executing here.
    let Ok(Answer::Repeat(earlier)) = engine.request(&request) else {
        panic!("not answered as a repeat");
    };
    assert_eq!(earlier.state(), &RunState::Ended(outcome));

    // The executor goes on: the next call runs.
    let world = World::new();
    let engine = world.engine(vec![pair(|_, _| std::panic::panic_any(7_u32))]);
    let finished = engine.submit(&Call::new("pair")).unwrap();
    assert_eq!(
        finished.outcome(),
        &Outcome::Error {
            kind: "panic".to_owned(),
            message: "the action panicked, and its panic carried no text".to_owned(),
        }
    );
    let again = engine.submit(&Call::new("pair")).unwrap();
    assert_ne!(again.run(), finished.run());
}
