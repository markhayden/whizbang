//! `grow` as an action, run through the executor against a store: it commits a mask
//! whose origin names the action, its arguments and its input; the same inputs give the
//! same identity; a call it cannot take is refused before any run exists; it stops
//! when its run is cancelled; and a failure of the store while it runs leaves no path
//! in its record.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use whizbang_core::artifact::{self, Artifact, ArtifactId, ArtifactType, Mask, Step};
use whizbang_core::contract::record::{self, DIAGNOSTICS_PREFIX, JOURNAL_NAME, RunRecord, Status};
use whizbang_core::contract::{
    Action, ActionDescriptor, Call, CallError, Context, Count, Engine, Executor, Failure,
    InputFault, Kind, NoBackends, Outcome, PreparedCall, Produced, Refusal, Registry, Value,
    ValueType, read_journal,
};
use whizbang_core::identity::{DecodeLimits, Digest};
use whizbang_core::store::fs::Fs;
use whizbang_core::store::{self, Reader, Store};
use whizbang_rascal::grow::{DIAGNOSTIC, Grow, NAME, PX_MAX, VERSION};
use whizbang_testkit::fs_shim::ShimFs;
use whizbang_testkit::scratch::Scratch;

use super::grow::{VECTORS, made_by, mask_of, on, plane, vector_mask, vector_result};

/// `grow` with something of the test's done before it runs and after it has come back:
/// the same descriptor, and the same code in between.
pub struct Around {
    grow: Grow,
    before: Box<dyn Fn(&Context) + Send + Sync>,
    after: Box<Hand>,
}

type Hand = dyn Fn(Result<Produced, Failure>, &Context) -> Result<Produced, Failure> + Send + Sync;

impl Around {
    pub fn before(before: impl Fn(&Context) + Send + Sync + 'static) -> Around {
        Around {
            grow: Grow::new(),
            before: Box::new(before),
            after: Box::new(|handed, _| handed),
        }
    }
}

impl Action for Around {
    fn descriptor(&self) -> &ActionDescriptor {
        self.grow.descriptor()
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        (self.before)(context);
        (self.after)(self.grow.run(call, context), context)
    }
}

/// A store in a folder of its own, on a file system that can lose power.
pub struct World {
    dir: Scratch,
    pub fs: Arc<ShimFs>,
}

impl World {
    pub fn new() -> World {
        World::on(ShimFs::new())
    }

    pub fn on(fs: ShimFs) -> World {
        World {
            dir: Scratch::new("grow"),
            fs: Arc::new(fs),
        }
    }

    pub fn root(&self) -> PathBuf {
        self.dir.path().join("store")
    }

    pub fn store(&self) -> Arc<Store> {
        Arc::new(Store::open(&self.root(), self.fs.clone()).unwrap())
    }

    /// Brings a mask into the store, as `whizbang store import` does.
    pub fn import(&self, mask: &Mask) -> ArtifactId {
        let store = self.store();
        let mut publication = store.begin().unwrap();
        let id = artifact::put(&mut publication, &Artifact::from(mask.clone())).unwrap();
        publication.publish().unwrap();
        id
    }

    /// The executor over this store with `grow` as the build ships it.
    pub fn engine(&self) -> Engine {
        engine_over(self.store(), Arc::new(Grow::new()))
    }
}

pub fn engine_over(store: Arc<Store>, grow: Arc<dyn Action>) -> Engine {
    let mut registry = Registry::new();
    registry.register(grow).unwrap();
    Engine::over(registry, Arc::new(NoBackends), store, Digest::of_bytes(b""))
}

pub fn call(mask: ArtifactId, px: f64, down_px: Option<f64>) -> Call {
    let call = Call::new("grow")
        .argument("px", Value::Number(px))
        .input("mask", mask);
    match down_px {
        Some(down_px) => call.argument("down_px", Value::Number(down_px)),
        None => call,
    }
}

/// One run as a store holds it: its id, its record when it has one, every name it
/// holds with what is read through it, and the names that are committed outputs.
pub struct HeldRun {
    pub id: String,
    pub record: Option<RunRecord>,
    pub names: Vec<(String, Vec<u8>)>,
    pub outputs: Vec<String>,
}

impl HeldRun {
    /// What the run kept under this diagnostic name.
    pub fn diagnostic(&self, name: &str) -> Option<&[u8]> {
        let name = format!("{DIAGNOSTICS_PREFIX}{name}");
        let found = self.names.iter().find(|(held, _)| *held == name);
        found.map(|(_, bytes)| bytes.as_slice())
    }

    /// The messages of the run's journal, in order.
    pub fn journal(&self) -> Vec<String> {
        let found = self.names.iter().find(|(held, _)| held == JOURNAL_NAME);
        let lines = read_journal(&found.expect("a journal").1).unwrap();
        lines.iter().map(|line| line.message().to_owned()).collect()
    }
}

/// What a store holds, read by a reader of its own: its generation's number, its runs,
/// and each mask it holds under an artifact's name.
pub struct Held {
    pub generation: u64,
    pub runs: Vec<HeldRun>,
    pub masks: Vec<Mask>,
}

pub fn held(root: &Path, fs: Arc<dyn Fs>) -> Held {
    let reader = Reader::open(root, fs).unwrap();
    let generation = reader.snapshot().unwrap();
    let runs = generation
        .runs()
        .iter()
        .map(|(id, run)| {
            let stored = record::stored(&generation, id, |entry| reader.read(entry));
            let record = match stored.unwrap() {
                Ok(record) => Some(record),
                Err(record::StoredRecordError::NoRecord) => None,
                Err(other) => panic!("{other}"),
            };
            let names = run.entries().iter();
            let outputs = names
                .clone()
                .filter(|(_, entry)| entry.kind == store::Kind::Output)
                .map(|(name, _)| name.clone())
                .collect();
            HeldRun {
                id: id.clone(),
                record,
                names: names
                    .map(|(name, entry)| (name.clone(), reader.read(entry).unwrap()))
                    .collect(),
                outputs,
            }
        })
        .collect();
    let masks = generation
        .entries()
        .iter()
        .filter(|(name, _)| name.starts_with("artifacts/mask/"))
        // A test may have damaged a file: what cannot be read is not held.
        .filter_map(|(_, entry)| reader.read_output(entry).ok())
        .map(|bytes| Mask::from_bytes(&bytes, &DecodeLimits::default()).unwrap())
        .collect();
    Held {
        generation: generation.number(),
        runs,
        masks,
    }
}

/// A mask of a 24 by 16 image: a block of core, and one protected column beside it.
pub fn fixture() -> Mask {
    mask_of(
        24,
        16,
        plane(24, 16, |x, y| (8..12).contains(&x) && (5..9).contains(&y)),
        &plane(24, 16, |x, _| x == 14),
    )
}

// --- what it is -------------------------------------------------------------------------------

#[test]
fn the_descriptor_says_what_grow_takes_and_returns() {
    let grow = Grow::new();
    let descriptor = grow.descriptor();
    assert_eq!((descriptor.name(), descriptor.version()), (NAME, VERSION));
    assert_eq!(descriptor.kind(), Kind::Primitive);
    let within = ValueType::number(Some(0.0), Some(PX_MAX));
    let arguments: Vec<_> = descriptor
        .arguments()
        .iter()
        .map(|a| {
            (
                a.name(),
                a.is_required(),
                a.default().is_none(),
                a.value_type(),
            )
        })
        .collect();
    assert_eq!(
        arguments,
        [
            ("px", true, true, &within),
            ("down_px", false, true, &within)
        ]
    );
    assert_eq!(PX_MAX, 10_000.0);
    let inputs: Vec<_> = descriptor
        .inputs()
        .iter()
        .map(|port| (port.name(), port.artifact(), port.count()))
        .collect();
    assert_eq!(inputs, [("mask", ArtifactType::Mask, Count::exactly(1))]);
    let outputs: Vec<_> = descriptor
        .outputs()
        .iter()
        .map(|port| (port.name(), port.artifact(), port.count()))
        .collect();
    assert_eq!(outputs, [("mask", ArtifactType::Mask, Count::exactly(1))]);
    // It calls no model.
    let world = World::new();
    let input = world.import(&fixture());
    let prepared = world.engine().prepare(&call(input, 8.0, None));
    assert!(prepared.unwrap().needs().is_empty());
}

// --- a run ------------------------------------------------------------------------------------

#[test]
fn grow_commits_a_mask_whose_origin_names_the_action_its_arguments_and_its_input() {
    let world = World::new();
    let input = world.import(&fixture());
    let engine = world.engine();
    let finished = engine.submit(&call(input, 2.5, Some(3.0))).unwrap();
    let Outcome::Completed { outputs } = finished.outcome() else {
        panic!("{:?}", finished.outcome());
    };
    let [output] = outputs.as_slice() else {
        panic!("{outputs:?}");
    };
    assert_eq!(
        (output.port(), output.artifact()),
        ("mask", ArtifactType::Mask)
    );
    assert_ne!(output.id(), input);
    drop(engine);

    // The store holds the mask under its type and identity, as a committed output, and
    // the run holds its own reference to the same file.
    let after = held(&world.root(), world.fs.clone());
    let mask = after.masks.iter().find(|mask| mask.id() == output.id());
    let mask = mask.expect("the grown mask");
    let [run] = after.runs.as_slice() else {
        panic!("{} runs", after.runs.len());
    };
    assert_eq!(run.id, finished.run().as_str());
    assert_eq!(run.outputs, ["outputs/mask/0"]);
    let through = run.names.iter().find(|(name, _)| name == "outputs/mask/0");
    assert_eq!(through.unwrap().1, mask.to_bytes());

    // Its origin: the fixture's own step, then this one.
    assert_eq!(
        mask.origin().steps(),
        [
            Step::new("fixture", "1"),
            Step::new("grow", "1.0.0")
                .argument("px", "2.5")
                .argument("down_px", "3")
                .input("mask", input.0),
        ]
    );
    // Its core: the block, columns 8 to 11 and rows 5 to 8, grown by 2.5 (K is 6: two
    // whole pixels each way, and the corners at one and one, one and two, two and one),
    // then 3 rows down; never in the protected column 14, which it does not reach.
    let before = fixture();
    let expected = plane(24, 16, |x, y| {
        (0..=3.min(y)).any(|up| {
            let beside = |at: usize, first: usize, last: usize| {
                (first.saturating_sub(at) + at.saturating_sub(last)) as i64
            };
            let (dx, dy) = (beside(x, 8, 11), beside(y - up, 5, 8));
            dx * dx + dy * dy <= 6
        })
    });
    assert_eq!(mask.core(), expected);
    assert_eq!(mask.protect(), before.protect());
    assert_eq!(mask.binding(), before.binding());

    // The record: completed, what was asked, the input and the output.
    let record = run.record.as_ref().expect("a record");
    assert_eq!(record.outcome().status, Status::Completed);
    assert_eq!(
        (
            record.asked().action.as_str(),
            record.asked().version.as_str()
        ),
        ("grow", "1.0.0")
    );
    let listed: Vec<_> = record
        .inputs()
        .iter()
        .map(|i| (i.port.as_str(), i.artifact))
        .collect();
    assert_eq!(listed, [("mask", input.0)]);
    let listed: Vec<_> = record
        .outputs()
        .iter()
        .map(|o| (o.port.as_str(), o.artifact))
        .collect();
    assert_eq!(listed, [("mask", output.id().0)]);
    // What it kept as evidence: the numbers the rule ran with, and what it reported.
    assert_eq!(
        run.diagnostic(DIAGNOSTIC)
            .map(String::from_utf8_lossy)
            .as_deref(),
        Some("{\"px\":\"2.5\",\"down_px\":\"3\",\"radius_squared\":6,\"rows_down\":3}\n")
    );
    assert_eq!(
        run.journal(),
        [
            "grow: read the mask",
            "grow: the core went from 16 to 84 pixels; the blend is cleared"
        ]
    );
    assert_eq!(
        mask.core().iter().filter(|sample| **sample != 0).count(),
        84
    );
}

#[test]
fn the_same_inputs_give_the_same_artifact_identity() {
    // In one store, twice; and in another store. One identity, and one file under it.
    let first = World::new();
    let input = first.import(&fixture());
    let engine = first.engine();
    let ids: Vec<ArtifactId> = (0..2)
        .map(|_| {
            let finished = engine.submit(&call(input, 2.5, Some(3.0))).unwrap();
            finished.outcome().outputs()[0].id()
        })
        .collect();
    let second = World::new();
    assert_eq!(second.import(&fixture()), input);
    let finished = second
        .engine()
        .submit(&call(input, 2.5, Some(3.0)))
        .unwrap();
    assert_eq!(finished.outcome().outputs()[0].id(), ids[0]);
    assert_eq!(ids[0], ids[1]);
    drop(engine);
    let after = held(&first.root(), first.fs.clone());
    assert_eq!(after.runs.len(), 2);
    assert_eq!(after.masks.len(), 2);

    // Another radius is another mask.
    let other = second
        .engine()
        .submit(&call(input, 2.0, Some(3.0)))
        .unwrap();
    assert_ne!(other.outcome().outputs()[0].id(), ids[0]);

    // The identity itself, fixed: it is the same on every machine, because nothing in
    // the rule is decided by a comparison of doubles. And the whole file, by its digest,
    // which holds the origin with the arguments as they are written.
    let through = |vector: usize| {
        let vector = &VECTORS[vector];
        let (.., px, down_px, _, _) = *vector;
        let world = World::new();
        let input = world.import(&vector_mask(vector));
        let finished = world
            .engine()
            .submit(&call(input, px, Some(down_px)))
            .unwrap();
        let id = finished.outcome().outputs()[0].id();
        let after = held(&world.root(), world.fs.clone());
        let mask = after
            .masks
            .into_iter()
            .find(|mask| mask.id() == id)
            .unwrap();
        // Through the executor, the core is the one the vector fixes.
        assert_eq!(
            Digest::of_bytes(mask.core()).to_string(),
            vector_result(vector).1
        );
        (
            id.0.to_string(),
            Digest::of_bytes(&mask.to_bytes()).to_string(),
        )
    };
    assert_eq!(
        through(6),
        (
            "sha256:19abaf4486efbe3e320051347d198bd16f7679b2a9339d059a92cf23de3fed0a".to_owned(),
            "sha256:b42bfd2455a848283b42ae67f74e396d6259263316427a704e697da256e14fea".to_owned()
        )
    );
    assert_eq!(
        through(11),
        (
            "sha256:64e8a046d36f11ab4b0030ff36c5f31c81f6240fb5956641bac283df3bb89269".to_owned(),
            "sha256:fe3fea83a90ae81f6cd040c3ee47cf68ecc78ea1370dc346469c3794a31e3e64".to_owned()
        )
    );
}

// --- what it refuses --------------------------------------------------------------------------

#[test]
fn a_call_grow_cannot_take_is_refused_before_any_run_exists() {
    let world = World::new();
    let input = world.import(&fixture());
    let store = world.store();
    let engine = engine_over(store.clone(), Arc::new(Grow::new()));
    let generation = held(&world.root(), world.fs.clone()).generation;
    let refused = |call: &Call| -> String {
        // Prepare and submit say the same, and neither makes a run.
        let prepared = engine.prepare(call).unwrap_err();
        let submitted = engine.submit(call).unwrap_err();
        assert_eq!(prepared, submitted);
        assert!(
            matches!(
                prepared,
                CallError::Refused(Refusal::Invalid { ref action, .. }) if action == "grow"
            ),
            "{prepared:?}"
        );
        prepared.to_string()
    };
    let with = |name: &str, value: Value| {
        let base = Call::new("grow").input("mask", input);
        let base = if name == "px" {
            base
        } else {
            base.argument("px", Value::Number(8.0))
        };
        base.argument(name, value)
    };
    for name in ["px", "down_px"] {
        for (value, why) in [
            (-1.0, "-1 is outside 0 to 10000"),
            (-1e-300, "is outside 0 to 10000"),
            (10_000.000_000_000_002, "is outside 0 to 10000"),
            (1e300, "is outside 0 to 10000"),
            (f64::NAN, "NaN is not a finite number"),
            (f64::INFINITY, "inf is not a finite number"),
            (f64::NEG_INFINITY, "-inf is not a finite number"),
        ] {
            let said = refused(&with(name, Value::Number(value)));
            assert!(
                said.contains(name) && said.contains(why),
                "{name} {value}: {said}"
            );
        }
        // A whole number and a text are not numbers: a door decides what was meant.
        let said = refused(&with(name, Value::Integer(8)));
        assert!(
            said.contains("a whole number where a number is required"),
            "{said}"
        );
        let said = refused(&with(name, Value::text("8")));
        assert!(said.contains("a text where a number is required"), "{said}");
    }
    // No radius, an argument it does not have, no mask, and two masks.
    let said = refused(&Call::new("grow").input("mask", input));
    assert!(said.contains("px"), "{said}");
    let said = refused(&call(input, 8.0, None).argument("method", Value::text("fast")));
    assert!(said.contains("method"), "{said}");
    let said = refused(&Call::new("grow").argument("px", Value::Number(8.0)));
    assert!(said.contains("mask"), "{said}");
    let said = refused(&call(input, 8.0, None).input("mask", input));
    assert!(said.contains("mask"), "{said}");

    // A mask the store does not hold, and something that is not a mask.
    let absent = ArtifactId(Digest::of_bytes(b"no such mask"));
    assert!(matches!(
        engine.submit(&call(absent, 8.0, None)),
        Err(CallError::Refused(Refusal::Input {
            fault: InputFault::Missing,
            ..
        }))
    ));
    let line = whizbang_core::artifact::Location::new(
        on(24, 16),
        whizbang_core::artifact::Shape::Line {
            line: whizbang_core::artifact::Line::Column(4.0),
            width: 3.0,
        },
        made_by("line"),
    )
    .unwrap();
    let mut publication = store.begin().unwrap();
    let line = artifact::put(&mut publication, &Artifact::from(line)).unwrap();
    publication.publish().unwrap();
    assert!(matches!(
        engine.submit(&call(line, 8.0, None)),
        Err(CallError::Refused(Refusal::Input {
            fault: InputFault::WrongType { .. },
            ..
        }))
    ));

    // Nothing ran: the store holds no run, and only the line was published since.
    let after = held(&world.root(), world.fs.clone());
    assert!(after.runs.is_empty());
    assert_eq!(after.generation, generation + 1);

    // The ends of the range are in it, and zero grows nothing.
    for (px, down_px) in [(0.0, Some(0.0)), (-0.0, None), (10_000.0, Some(10_000.0))] {
        let finished = engine.submit(&call(input, px, down_px)).unwrap();
        assert_eq!(finished.outcome().name(), "completed", "px {px}");
    }
    let after = held(&world.root(), world.fs.clone());
    // By nothing, the fixture comes back with the identity it had; by the most, every
    // pixel that is not protected is core.
    assert_eq!(
        after.runs[0].record.as_ref().unwrap().outputs()[0].artifact,
        input.0
    );
    let full = after
        .masks
        .iter()
        .find(|mask| mask.core().iter().filter(|sample| **sample != 0).count() == 24 * 16 - 16);
    assert!(full.is_some());
}

// --- stopping, and failing --------------------------------------------------------------------

#[test]
fn grow_stops_when_its_run_is_cancelled_and_commits_nothing() {
    // The run is cancelled, with its own signal, just before `grow` starts on it: an
    // accepted cancellation, on one thread. `grow` sees it at its first look and comes
    // back without a mask.
    let world = World::new();
    let input = world.import(&fixture());
    let came_back = Arc::new(std::sync::Mutex::new(Vec::new()));
    let around = {
        let came_back = came_back.clone();
        Around {
            grow: Grow::new(),
            before: Box::new(|context| context.cancellation().cancel()),
            after: Box::new(move |handed, _| {
                came_back.lock().unwrap().push(handed.clone());
                handed
            }),
        }
    };
    let engine = engine_over(world.store(), Arc::new(around));
    let finished = engine.submit(&call(input, 8.0, Some(4.0))).unwrap();
    assert_eq!(finished.outcome(), &Outcome::Cancelled);
    assert_eq!(
        *came_back.lock().unwrap(),
        [Err(Failure::error(
            "cancelled",
            "grow stopped: its run was cancelled"
        ))]
    );
    drop(engine);
    let after = held(&world.root(), world.fs.clone());
    assert_eq!(after.masks.len(), 1);
    let record = after.runs[0].record.as_ref().unwrap();
    assert_eq!(record.outcome().status, Status::Cancelled);
    assert!(record.outputs().is_empty() && after.runs[0].outputs.is_empty());
    // What it wrote before it stopped is kept.
    assert!(after.runs[0].diagnostic(DIAGNOSTIC).is_some());
    assert_eq!(after.runs[0].journal(), ["grow: read the mask"]);
}

/// Every file under a folder.
fn files_under(folder: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(folder).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            found.extend(files_under(&path));
        } else {
            found.push(path);
        }
    }
    found
}

#[test]
fn a_failure_of_the_store_inside_a_running_grow_leaves_no_path_in_its_record() {
    // The mask's file is damaged after the call was admitted and before `grow` reads
    // it: the store's own words for that name the file by its whole path.
    let world = World::new();
    let input = world.import(&fixture());
    let root = world.root();
    let length = Artifact::from(fixture()).to_bytes().len() as u64;
    let damage = {
        let root = root.clone();
        move |_: &Context| {
            let objects = files_under(&root.join("objects"));
            let masks = objects
                .iter()
                .filter(|path| path.metadata().unwrap().len() == length);
            let masks: Vec<_> = masks.collect();
            assert_eq!(masks.len(), 1, "{objects:?}");
            std::fs::write(masks[0], b"not a mask").unwrap();
        }
    };
    let engine = engine_over(world.store(), Arc::new(Around::before(damage)));
    let finished = engine.submit(&call(input, 8.0, None)).unwrap();
    let Outcome::Error { kind, message } = finished.outcome() else {
        panic!("{:?}", finished.outcome());
    };
    assert_eq!(kind, "input");
    assert!(
        message.starts_with("the store's file ") && message.ends_with(" is missing or damaged"),
        "{message}"
    );
    drop(engine);

    // Nowhere in what the run kept is the store's place, or any part of it.
    let after = held(&root, world.fs.clone());
    let [run] = after.runs.as_slice() else {
        panic!("{} runs", after.runs.len());
    };
    let folder = root
        .parent()
        .unwrap()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap();
    for (name, bytes) in &run.names {
        let text = String::from_utf8_lossy(bytes);
        assert!(!text.contains(root.to_str().unwrap()), "{name}: {text}");
        assert!(
            !text.contains(folder) && !text.contains("objects"),
            "{name}: {text}"
        );
    }
    let record = run.record.as_ref().unwrap();
    assert_eq!(record.outcome().status, Status::Error);
    assert!(record.outputs().is_empty() && run.outputs.is_empty());
    // The diagnostic it wrote before it read its input is kept, and readable.
    assert_eq!(
        run.diagnostic(DIAGNOSTIC)
            .map(String::from_utf8_lossy)
            .as_deref(),
        Some("{\"px\":\"8\",\"down_px\":null,\"radius_squared\":64,\"rows_down\":0}\n")
    );
    assert!(run.journal().is_empty());
}
