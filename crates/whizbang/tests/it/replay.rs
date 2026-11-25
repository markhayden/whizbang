//! `replay::…`: replay (plan m7, task R1; ADR 0049), through the binary's own runner over
//! stores the loop's kit of fakes filled (`checked::Looped`): a removal (`remove_prompt`)
//! and its check, each replayed offline with recorded handles given to that one run and
//! its descendants, and nothing else. Nothing here sends anything anywhere: the kit's
//! backends are fakes, and an offline replay calls none of them.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use whizbang::wiring::{self, Over};
use whizbang_core::capability::{Controls, JudgeRequest, Message, Part, Role};
use whizbang_core::contract::record::{self, ReplayMode, Replayed, RunRecord};
use whizbang_core::contract::replay::{Replay, Source};
use whizbang_core::contract::{
    ADMITTED_NAME, Action, ActionDescriptor, Admission, Answer, Call, CallError, Cancel,
    Capability, Context, Engine, Executor, Failure, Kind, Outcome, PreparedCall, Produced, Refusal,
    RequestId, Requirement, RunId, RunState,
};
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Reader, Store};

use crate::bundle::{checking, removal};
use crate::checked::{Looped, Setup, output, passes};

/// A world on the real disk whose judge passes what it is asked `answers` times.
fn world(label: &str, answers: usize) -> Looped {
    Looped::new(label, &Setup::answering(vec![passes(); answers]).on_disk())
}

/// A runner over the world's store under no config at all: no backend is configured.
fn bare(world: &Looped) -> Engine {
    let store = Arc::new(Store::open(&world.root, Arc::new(RealFs)).unwrap());
    wiring::runner(wiring::registry().unwrap(), store, &Over::nothing()).unwrap()
}

/// Starts a replay and waits for it: its run and how it ended.
fn replayed(engine: &Engine, replay: &Replay) -> (RunId, Outcome) {
    let Answer::Started(run) = engine.start_replay(replay).unwrap() else {
        panic!("not started");
    };
    let waited = engine.wait(&run).unwrap();
    let RunState::Ended(outcome) = waited.state() else {
        panic!("{:?}", waited.state());
    };
    (run, outcome.clone())
}

fn of_run(run: &RunId) -> Replay {
    Replay::of(Source::Run(run.clone()))
}

/// A run's record, through a reader of the store on the real disk.
fn record(world: &Looped, run: &str) -> RunRecord {
    let reader = Reader::open(&world.root, Arc::new(RealFs)).unwrap();
    let generation = reader.snapshot().unwrap();
    record::stored(&generation, run, |entry| reader.read(entry))
        .unwrap()
        .unwrap()
}

/// A run's admission, as the store holds it.
fn admission(world: &Looped, run: &str) -> Admission {
    let reader = Reader::open(&world.root, Arc::new(RealFs)).unwrap();
    let generation = reader.snapshot().unwrap();
    let entry = generation.run(run).unwrap().get(ADMITTED_NAME).unwrap();
    Admission::from_bytes(&reader.read(entry).unwrap()).unwrap()
}

/// Every run of a family, the root first and each run before the runs it called.
fn family(world: &Looped, root: &str) -> Vec<RunRecord> {
    let mut runs = vec![record(world, root)];
    let mut at = 0;
    while at < runs.len() {
        let steps: Vec<String> = runs[at]
            .steps()
            .iter()
            .map(|s| s.run().to_owned())
            .collect();
        runs.extend(steps.iter().map(|step| record(world, step)));
        at += 1;
    }
    runs
}

/// Whether a run keeps the note of where its replay stopped, and what it says.
fn note(world: &Looped, run: &str) -> Option<String> {
    let reader = Reader::open(&world.root, Arc::new(RealFs)).unwrap();
    let generation = reader.snapshot().unwrap();
    let name = whizbang_core::contract::replay::note_name();
    let entry = generation.run(run)?.get(&name)?;
    Some(String::from_utf8(reader.read(entry).unwrap()).unwrap())
}

/// What every run of a replayed family names: the run it replays, offline.
fn offline_of(run: &RunId) -> Replayed {
    Replayed {
        run: run.as_str().to_owned(),
        bundle: None,
        mode: ReplayMode::Offline,
    }
}

#[test]
fn an_offline_replay_of_remove_prompt_reproduces_it_with_no_backend_configured() {
    // The names the core keeps of rascal's are rascal's.
    use whizbang_core::capability::recorded::{SEGMENT_ACTION, SEGMENT_DIAGNOSTIC};
    assert_eq!(SEGMENT_ACTION, whizbang_rascal::segment::NAME);
    assert_eq!(SEGMENT_DIAGNOSTIC, whizbang_rascal::segment::DIAGNOSTIC);
    let world = world("replay-remove", 1);
    let removed = removal(&world);
    let sent = world.kit.requests();
    // A runner under no config: no backend is configured, and the replay needs none.
    let engine = bare(&world);
    let (run, outcome) = replayed(&engine, &of_run(removed.run()));
    assert_eq!(
        &outcome,
        removed.outcome(),
        "the same outputs, by identity and port"
    );
    assert_ne!(&run, removed.run());
    // Nothing was asked of any backend.
    assert_eq!(world.kit.requests(), sent);
    assert_eq!(note(&world, run.as_str()), None);
    // Every run of the replayed family names the run it replays, a child as its root
    // does, in its record and in its admission.
    let replayed_family = family(&world, run.as_str());
    let original = family(&world, removed.run().as_str());
    assert_eq!(replayed_family.len(), original.len());
    assert!(replayed_family.len() > 1, "the removal calls its steps");
    for (made, was) in replayed_family.iter().zip(&original) {
        assert_eq!(made.replayed(), Some(&offline_of(removed.run())));
        assert_eq!(made.version(), record::RUN_7);
        assert_eq!(made.asked().action, was.asked().action);
        assert_eq!(made.outcome().status, was.outcome().status);
        let admitted = admission(&world, &made.identity().run);
        assert_eq!(admitted.replayed(), Some(&offline_of(removed.run())));
        // The recorded models answered: the same backends and models, nothing paid,
        // and no data went anywhere.
        let models = |r: &RunRecord| -> Vec<(Capability, String, String)> {
            (r.models().iter())
                .map(|m| (m.capability, m.backend.clone(), m.model.clone()))
                .collect()
        };
        assert_eq!(models(made), models(was));
        assert!(made.spend().requests.is_empty());
        assert!(made.egress().is_empty());
    }
    drop(engine);
}

#[test]
fn an_offline_replay_of_a_check_reproduces_its_turns_checklist_and_verdict() {
    let world = world("replay-check", 1);
    let removed = removal(&world);
    let checked = checking(&world, output(&removed, "image"));
    let sent = world.kit.requests();
    let engine = bare(&world);
    let (run, outcome) = replayed(&engine, &of_run(checked.run()));
    assert_eq!(&outcome, checked.outcome());
    assert_eq!(world.kit.requests(), sent);
    assert_eq!(note(&world, run.as_str()), None);
    let made = record(&world, run.as_str());
    let was = record(&world, checked.run().as_str());
    assert_eq!(made.verdict(), was.verdict());
    assert_eq!(made.findings(), was.findings());
    // The same turns: each exchange the replay's family stored is the one the original's
    // stored, run for run matched by place.
    let exchanges = |root: &RunId| -> Vec<(String, whizbang_core::identity::Digest)> {
        (family(&world, root.as_str()).iter())
            .flat_map(|r| r.diagnostics().to_vec())
            .filter(|d| d.name().starts_with("diagnostics/exchange-"))
            .map(|d| (d.name().to_owned(), d.digest()))
            .collect()
    };
    assert!(!exchanges(checked.run()).is_empty());
    assert_eq!(exchanges(&run), exchanges(checked.run()));
}

/// How many times `asking` has run: each run asks the judge a question that names it.
static ASKED: Mutex<u32> = Mutex::new(0);

/// An action that asks the judge one question, which says how many times it has run.
struct Asking(ActionDescriptor);

impl Action for Asking {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }

    fn run(&self, _: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let asked = {
            let mut asked = ASKED.lock().unwrap();
            *asked += 1;
            *asked
        };
        let judge = context.capabilities().judge().expect("a judge was handed");
        let text = format!("question {asked}");
        let message = Message::new(Role::User, vec![Part::Text(text)]).unwrap();
        let request = JudgeRequest::new(
            "a test",
            vec![message],
            Vec::new(),
            Controls::new(64).unwrap(),
        )
        .unwrap();
        match judge.send(&request) {
            Ok(_) => Ok(Produced::nothing()),
            Err(fault) => Err(Failure::error("judge", fault.sentence())),
        }
    }
}

fn asking() -> Arc<dyn Action> {
    let draft = ActionDescriptor::draft("asking", "1.0.0", Kind::Primitive, "A test action.")
        .needs(Requirement::new(Capability::Judge));
    Arc::new(Asking(draft.build().unwrap()))
}

/// The world's runner, with `asking` among its actions and among those it replays.
fn with_asking(world: &Looped) -> Engine {
    let mut registry = wiring::registry_over(&world.over, false).unwrap();
    registry.register(asking()).unwrap();
    let mut shipped = whizbang::bundling::shipped().unwrap();
    shipped.insert("asking".to_owned(), "1.0.0".to_owned());
    world.engine_of(registry).with_shipped(shipped)
}

#[test]
fn a_request_the_original_did_not_make_is_diverged_and_nothing_is_called() {
    let world = world("replay-diverged", 2);
    let engine = with_asking(&world);
    let original = engine.submit(&Call::new("asking")).unwrap();
    assert_eq!(
        original.outcome().name(),
        "completed",
        "{:?}",
        original.outcome()
    );
    let sent = world.kit.requests();
    // The replay asks another question: diverged, at the first judge request, and no
    // backend is called; the action's own failure is the action's.
    let (run, outcome) = replayed(&engine, &of_run(original.run()));
    assert_eq!(outcome.name(), "error");
    assert_eq!(world.kit.requests(), sent);
    let said = note(&world, run.as_str()).expect("the replay's stop is noted");
    assert!(
        said.starts_with("diverged judge request 1: the replay sent the judge another request"),
        "{said}"
    );
}

#[test]
fn a_request_id_under_another_mode_or_source_is_a_conflict_and_the_same_replay_returns_its_run() {
    let world = world("replay-ids", 1);
    let removed = removal(&world);
    let engine = bare(&world);
    let id = RequestId::parse("replay-one").unwrap();
    let replay = of_run(removed.run()).with_id(id.clone());
    let (run, _) = replayed(&engine, &replay);
    // The same replay again: its run, and nothing new.
    let Answer::Repeat(earlier) = engine.start_replay(&replay).unwrap() else {
        panic!("not a repeat");
    };
    assert_eq!(earlier.run(), &run);
    let conflict = |refused: CallError| {
        assert!(
            matches!(&refused, CallError::Refused(Refusal::Conflict { run: r, .. }) if *r == run),
            "{refused:?}"
        );
    };
    // Live, or of another source, or an ordinary call under the id: each a conflict.
    conflict(engine.start_replay(&replay.clone().live()).unwrap_err());
    conflict(
        engine
            .start_replay(&of_run(&run).with_id(id.clone()))
            .unwrap_err(),
    );
    let call = whizbang_core::contract::Request::new(Call::new("remove_prompt")).with_id(id);
    conflict(engine.start(&call).unwrap_err());
}

#[test]
fn a_repeated_replay_is_answered_after_its_source_was_deleted_and_after_the_config_changed() {
    let world = world("replay-repeat", 1);
    let removed = removal(&world);
    let id = RequestId::parse("replay-two").unwrap();
    let replay = of_run(removed.run()).with_id(id);
    let engine = world.engine();
    let (run, _) = replayed(&engine, &replay);
    drop(engine);
    // The source goes, and the config is another.
    let store = Store::open(&world.root, Arc::new(RealFs)).unwrap();
    store.delete(&[removed.run().as_str()]).unwrap();
    drop(store);
    let engine = bare(&world);
    let Answer::Repeat(earlier) = engine.start_replay(&replay).unwrap() else {
        panic!("not a repeat");
    };
    assert_eq!(earlier.run(), &run);
    // Without the id, the source is read, and is gone.
    let refused = engine.start_replay(&of_run(removed.run())).unwrap_err();
    assert!(
        matches!(refused, CallError::Refused(Refusal::NoSuchRun { .. })),
        "{refused:?}"
    );
}

/// Whether `waiting` waits for its cancellation, and where it says it has begun.
static WAITS: AtomicBool = AtomicBool::new(false);
static BEGUN: Mutex<Option<std::sync::mpsc::Sender<()>>> = Mutex::new(None);

/// An action that, when told to, says it has begun and waits for its cancellation.
struct Waiting(ActionDescriptor);

impl Action for Waiting {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }

    fn run(&self, _: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        if !WAITS.load(Ordering::SeqCst) {
            return Ok(Produced::nothing());
        }
        if let Some(begun) = BEGUN.lock().unwrap().take() {
            let _ = begun.send(());
        }
        // A bounded wait for the cancellation: 3000 looks of 10 ms at most.
        for _ in 0..3000 {
            if context.cancellation().is_cancelled() {
                return Err(Failure::error("cancelled", "the run was cancelled"));
            }
            std::thread::park_timeout(std::time::Duration::from_millis(10));
        }
        Err(Failure::error("never", "the run was never cancelled"))
    }
}

#[test]
fn a_replay_in_a_held_store_is_cancelled_waited_for_and_counted_like_any_run() {
    let world = world("replay-held", 1);
    let mut registry = wiring::registry_over(&world.over, false).unwrap();
    let draft = ActionDescriptor::draft("waiting", "1.0.0", Kind::Primitive, "A test action.");
    registry
        .register(Arc::new(Waiting(draft.build().unwrap())))
        .unwrap();
    let mut shipped = whizbang::bundling::shipped().unwrap();
    shipped.insert("waiting".to_owned(), "1.0.0".to_owned());
    let engine = world
        .engine_of(registry)
        .with_shipped(shipped)
        .with_started_bound(1)
        .unwrap();
    let original = engine.submit(&Call::new("waiting")).unwrap();
    assert_eq!(original.outcome().name(), "completed");
    // The replay waits for its cancellation, holding the one place there is.
    let (told, begun) = std::sync::mpsc::channel();
    *BEGUN.lock().unwrap() = Some(told);
    WAITS.store(true, Ordering::SeqCst);
    let Answer::Started(run) = engine.start_replay(&of_run(original.run())).unwrap() else {
        panic!("not started");
    };
    begun.recv().unwrap();
    assert_eq!(engine.on_threads(), 1);
    // Counted among the started runs: another start is refused at the bound.
    let refused = engine.start_replay(&of_run(original.run())).unwrap_err();
    assert!(
        matches!(
            refused,
            CallError::Refused(Refusal::TooManyStarted { bound: 1 })
        ),
        "{refused:?}"
    );
    assert_eq!(engine.cancel(&run).unwrap(), Cancel::Accepted);
    let waited = engine.wait(&run).unwrap();
    WAITS.store(false, Ordering::SeqCst);
    assert_eq!(waited.state(), &RunState::Ended(Outcome::Cancelled));
    assert_eq!(engine.on_threads(), 0);
    let made = record(&world, run.as_str());
    assert_eq!(made.replayed(), Some(&offline_of(original.run())));
}

#[test]
fn an_offline_replay_sends_nothing_reserves_nothing_reads_no_key_and_starts_no_worker() {
    use std::sync::atomic::Ordering;
    use whizbang_core::contract::Value;
    // A real runner over a models directory, whose filler is a local model of it: the
    // lease stands in its place for every ordinary run. The original asks that local
    // model; the model is then installed again, as another installation. An offline
    // replay takes no hold on any installation and asks no backend: its run reaches the
    // recorded answers, which here cannot answer (`paints` is no run of `fill`, which
    // alone keeps its answer), and the replay says so.
    let world = Arc::new(crate::wired::World::new("replay-lease"));
    let over = world.config("[backends.fill]\nkind = \"fake\"\n");
    let mut shipped = whizbang::bundling::shipped().unwrap();
    shipped.insert("paints".to_owned(), "1.0.0".to_owned());
    let runner = || {
        let store = Arc::new(Store::open(&world.root, Arc::new(RealFs)).unwrap());
        wiring::runner(world.registry(), store, &over)
            .unwrap()
            .with_shipped(shipped.clone())
    };
    let engine = runner();
    let original = engine
        .submit(&Call::new("paints").argument("times", Value::Integer(2)))
        .unwrap();
    assert_eq!(
        original.outcome().name(),
        "completed",
        "{:?}",
        original.outcome()
    );
    drop(engine);
    let (sent, ran) = (world.fakes.requests(), world.runs.load(Ordering::SeqCst));
    assert_eq!(sent, 2);
    world.install("2026-11.2");
    let engine = runner();
    let (run, _) = replayed(&engine, &of_run(original.run()));
    drop(engine);
    assert_eq!(world.fakes.requests(), sent, "no backend was asked");
    assert_eq!(world.runs.load(Ordering::SeqCst), ran + 1);
    let reader = Reader::open(&world.root, Arc::new(RealFs)).unwrap();
    let generation = reader.snapshot().unwrap();
    let name = whizbang_core::contract::replay::note_name();
    let entry = generation.run(run.as_str()).unwrap().get(&name).unwrap();
    let said = String::from_utf8(reader.read(entry).unwrap()).unwrap();
    assert!(said.starts_with("unanswerable fill request 1:"), "{said}");
    assert!(said.contains("only a run of fill"), "{said}");
    let made = record::stored(&generation, run.as_str(), |e| reader.read(e))
        .unwrap()
        .unwrap();
    let was = record::stored(&generation, original.run().as_str(), |e| reader.read(e))
        .unwrap()
        .unwrap();
    // Nothing reserved, nothing paid, nothing sent anywhere, no device named.
    assert_eq!(made.spend().reserved_micros, Some(0));
    assert!(made.spend().requests.is_empty());
    assert!(made.egress().is_empty());
    assert!(made.models().iter().all(|m| m.device.is_none()));
    // The recorded model, under the installation the original ran on.
    assert_eq!(made.models()[0].version, was.models()[0].version);
}

/// The build's actions with one changed: `name` is `with` given the action it replaces.
fn changing(
    name: &str,
    with: impl Fn(Arc<dyn Action>) -> Arc<dyn Action>,
) -> whizbang_core::contract::Registry {
    let built = wiring::registry().unwrap();
    let mut registry = whizbang_core::contract::Registry::new();
    let names: Vec<String> = built.descriptors().map(|d| d.name().to_owned()).collect();
    for each in names {
        let action = built.get(&each).unwrap().clone();
        registry
            .register(if each == name { with(action) } else { action })
            .unwrap();
    }
    registry
}

/// An action as `segment` or `fill` is described, whose run is `body`.
struct Asks(Arc<dyn Action>, AsksBody);

/// What an [`Asks`] runs.
type AsksBody = Box<dyn Fn(&PreparedCall, &Context) -> Result<Produced, Failure> + Send + Sync>;

impl Action for Asks {
    fn descriptor(&self) -> &ActionDescriptor {
        self.0.descriptor()
    }
    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        (self.1)(call, context)
    }
}

/// The picture at a port of a call.
fn picture_at(
    call: &PreparedCall,
    context: &Context,
    port: &str,
) -> (
    whizbang_core::artifact::Image,
    whizbang_core::capability::Picture,
) {
    use whizbang_core::artifact::{self, Artifact, ArtifactType};
    let limits = whizbang_core::identity::DecodeLimits::default();
    let input = call.inputs().iter().find(|i| i.port() == port).unwrap();
    let Ok(Artifact::Image(image)) =
        artifact::get(context.store(), ArtifactType::Image, input.id(), &limits)
    else {
        panic!("no image");
    };
    let pixels = image.pixels(&limits).unwrap();
    let picture = whizbang_core::capability::Picture::new(pixels).unwrap();
    (image, picture)
}

#[test]
fn a_segment_request_with_another_query_is_diverged_and_nothing_is_called() {
    use whizbang_core::capability::{Query, SegmentRequest};
    let world = world("replay-segment-diverged", 1);
    let removed = removal(&world);
    let sent = world.kit.requests();
    let registry = changing("segment", |inner| {
        Arc::new(Asks(
            inner,
            Box::new(|call, context| {
                let (image, picture) = picture_at(call, context, "image");
                let query = Query::phrase("something else").unwrap();
                let request = SegmentRequest::new(picture, vec![query], 8)
                    .and_then(|r| r.under(&image.binding()))
                    .unwrap();
                let segment = context.capabilities().segment().unwrap();
                match segment.segment(&request) {
                    Ok(_) => Ok(Produced::nothing()),
                    Err(fault) => Err(Failure::error("segment", fault.sentence())),
                }
            }),
        ))
    });
    let (run, _) = replayed(&world.engine_of(registry), &of_run(removed.run()));
    assert_eq!(world.kit.requests(), sent);
    let said = note(&world, run.as_str()).expect("stopped");
    assert!(
        said.starts_with(
            "diverged segment request 1: the replay asked a segmenter with another query"
        ),
        "{said}"
    );
}

#[test]
fn a_fill_request_with_another_prompt_is_diverged_and_nothing_is_called() {
    use whizbang_core::artifact::{self, ArtifactType};
    use whizbang_core::capability::{FillRequest, Support};
    let world = world("replay-fill-diverged", 1);
    let removed = removal(&world);
    let sent = world.kit.requests();
    let registry = changing("fill", |inner| {
        Arc::new(Asks(
            inner,
            Box::new(|call, context| {
                let (_, picture) = picture_at(call, context, "image");
                let limits = whizbang_core::identity::DecodeLimits::default();
                let input = call.inputs().iter().find(|i| i.port() == "mask").unwrap();
                let mask = artifact::get(context.store(), ArtifactType::Mask, input.id(), &limits)
                    .unwrap()
                    .into_mask()
                    .unwrap();
                let request =
                    FillRequest::new(picture, Support::of(&mask), Some("another prompt")).unwrap();
                let fill = context.capabilities().fill().unwrap();
                match fill.fill(&request) {
                    Ok(_) => Ok(Produced::nothing()),
                    Err(fault) => Err(Failure::error("fill", fault.sentence())),
                }
            }),
        ))
    });
    let (run, _) = replayed(&world.engine_of(registry), &of_run(removed.run()));
    assert_eq!(world.kit.requests(), sent);
    let said = note(&world, run.as_str()).expect("stopped");
    assert!(
        said.starts_with("diverged fill request 1: the replay asked a filler with another prompt"),
        "{said}"
    );
}

#[test]
fn the_query_the_core_makes_of_a_segment_run_is_segments_own() {
    use whizbang_core::capability::recorded::query_of;
    for (kind, text) in [("phrase", "a cat"), ("point", "3, 4"), ("box", "1 2 30 40")] {
        assert_eq!(
            query_of(kind, text),
            whizbang_rascal::segment::query_of(kind, text).ok(),
            "{kind} {text}"
        );
    }
}
