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

// --- the second part: the comparison, the command, live replay, bundles and stats ---------

struct Ran {
    code: Option<i32>,
    out: String,
    err: String,
}

/// The real binary with these arguments.
fn whizbang(args: &[&str]) -> Ran {
    let out = crate::wired::binary()
        .args(args)
        .output()
        .expect("the binary runs");
    Ran {
        code: out.status.code(),
        out: String::from_utf8(out.stdout).expect("output is UTF-8"),
        err: String::from_utf8(out.stderr).expect("output is UTF-8"),
    }
}

fn path(path: &std::path::Path) -> &str {
    path.to_str().unwrap()
}

/// The report of a replayed run, as the command reads it.
fn reported(world: &Looped, run: &RunId) -> whizbang::replaying::Reported {
    whizbang::replaying::report_of(&world.root, run.as_str()).unwrap()
}

#[test]
fn an_offline_replay_of_a_fill_through_a_feathered_mask_reproduces_its_picture() {
    let world = world("replay-fill", 1);
    let removed = removal(&world);
    let (run, _) = replayed(&bare(&world), &of_run(removed.run()));
    let report = reported(&world, &run);
    assert!(report.reproduced(), "{}", report.text());
    // The fill, whose mask the recipe feathered first, painted from its kept answer:
    // the same picture.
    let fill = |root: &str| {
        let family = family(&world, root);
        let fill = family
            .iter()
            .find(|r| r.asked().action.as_deref() == Some("fill"));
        fill.unwrap().outputs().to_vec()
    };
    let steps: Vec<String> = (family(&world, removed.run().as_str()).iter())
        .filter_map(|r| r.asked().action.clone())
        .collect();
    assert!(steps.iter().any(|action| action == "feather"), "{steps:?}");
    assert_eq!(fill(run.as_str()), fill(removed.run().as_str()));
}

#[test]
fn an_offline_replay_of_remove_checked_reproduces_its_attempts_and_verdict_through_the_run_match() {
    let world = world("replay-loop", 2);
    let original = world.run(&world.call());
    assert_eq!(
        original.outcome().name(),
        "completed",
        "{:?}",
        original.outcome()
    );
    let sent = world.kit.requests();
    let (run, outcome) = replayed(&bare(&world), &of_run(original.run()));
    assert_eq!(&outcome, original.outcome());
    assert_eq!(world.kit.requests(), sent);
    let report = reported(&world, &run);
    assert!(report.reproduced(), "{}", report.text());
    let (made, was) = (
        record(&world, run.as_str()),
        record(&world, original.run().as_str()),
    );
    let unpaid = |r: &RunRecord| {
        let attempted = r.attempts().unwrap();
        let verdicts: Vec<_> = (attempted.list.iter())
            .map(|a| (a.attempt, a.verdict))
            .collect();
        (verdicts, attempted.kept, attempted.ended.clone())
    };
    assert_eq!(unpaid(&made), unpaid(&was));
    assert_eq!(made.verdict(), was.verdict());
}

#[test]
fn an_offline_replay_through_the_command_reads_no_config_and_sends_and_reserves_nothing() {
    let world = world("replay-nothing", 1);
    let removed = removal(&world);
    let sent = world.kit.requests();
    let reservations = |world: &Looped| -> usize {
        let reader = Reader::open(&world.root, Arc::new(RealFs)).unwrap();
        let generation = reader.snapshot().unwrap();
        (generation.entries().keys())
            .filter(|name| name.starts_with(whizbang_core::contract::RESERVATIONS_PREFIX))
            .count()
    };
    let before = reservations(&world);
    assert!(before > 0, "the original reserved what it could spend");
    // Through the command, offline: it reads no config at all, so none is given, and
    // no env file, key or models directory is named.
    let said = whizbang(&[
        "runs",
        "replay",
        removed.run().as_str(),
        "--store",
        path(&world.root),
    ]);
    assert_eq!(said.code, Some(0), "{}", said.err);
    assert!(said.out.contains("report: reproduced"), "{}", said.out);
    assert_eq!(world.kit.requests(), sent, "nothing was sent");
    assert_eq!(reservations(&world), before, "nothing was reserved");
    let replay = said
        .out
        .lines()
        .next()
        .unwrap()
        .strip_prefix("replay: ")
        .unwrap();
    for made in family(&world, replay) {
        assert!(made.spend().requests.is_empty());
        assert_eq!(made.spend().reserved_micros, Some(0));
        assert!(made.egress().is_empty());
        // No worker answered: no request names a device.
        assert!(made.models().iter().all(|model| model.device.is_none()));
    }
}

#[test]
fn a_replay_neither_reads_nor_writes_the_decision_cache() {
    let setup = Setup {
        cached: true,
        ..Setup::answering(vec![passes(); 2])
    };
    let world = Looped::new("replay-cache", &setup.on_disk());
    let removed = removal(&world);
    let checked = checking(&world, output(&removed, "image"));
    let cache = |r: &RunRecord| -> Option<String> {
        (r.asked().settings.iter())
            .find(|s| s.name() == "decision_cache")
            .map(|s| format!("{:?}", s.value()))
    };
    let checks = |root: &str| -> Vec<RunRecord> {
        (family(&world, root).into_iter())
            .filter(|r| r.asked().action.as_deref() == Some("check"))
            .collect()
    };
    assert!(
        checks(checked.run().as_str())
            .iter()
            .any(|r| cache(r).is_some())
    );
    let sent = world.kit.requests();
    // A runner with the cache on: the replay neither reads it (the check is answered
    // from its own recorded turns, not the entry the original wrote) nor writes it.
    let (run, _) = replayed(&world.engine(), &of_run(checked.run()));
    assert_eq!(world.kit.requests(), sent);
    let report = reported(&world, &run);
    for check in checks(run.as_str()) {
        assert_eq!(cache(&check), None, "{}", report.text());
    }
}

#[test]
fn a_check_answered_from_the_cache_is_unanswerable_offline_and_asks_afresh_live() {
    let setup = Setup {
        cached: true,
        ..Setup::answering(vec![passes(); 2])
    };
    let world = Looped::new("replay-cached", &setup.on_disk());
    let removed = removal(&world);
    let image = output(&removed, "image");
    let _first = checking(&world, image);
    let hit = checking(&world, image);
    let sent = world.kit.requests();
    let (run, _) = replayed(&world.engine(), &of_run(hit.run()));
    let report = reported(&world, &run);
    assert_eq!(report.report.name(), "unanswerable", "{}", report.text());
    assert!(
        report.text().contains("decision cache"),
        "{}",
        report.text()
    );
    assert_eq!(world.kit.requests(), sent);
    // Live, the judge is asked afresh.
    let (_, outcome) = replayed(&world.engine(), &of_run(hit.run()).live());
    assert_eq!(outcome.name(), "completed", "{outcome:?}");
    assert!(world.kit.requests().2 > sent.2, "the judge was asked");
}

/// A record and a store that hides one diagnostic of one run: a store before B1 kept it.
struct Hiding<'a> {
    world: &'a Looped,
    hidden: String,
}

impl whizbang_core::capability::recorded::Held for Hiding<'_> {
    fn diagnostic(&self, run: &str, name: &str) -> Result<Option<Vec<u8>>, String> {
        if name == self.hidden {
            return Ok(None);
        }
        let reader = Reader::open(&self.world.root, Arc::new(RealFs)).unwrap();
        let generation = reader.snapshot().unwrap();
        let entry = generation.run(run).and_then(|held| held.get(name));
        Ok(entry.map(|entry| reader.read(entry).unwrap()))
    }

    fn input(&self, _: &str, _: &str, _: &str, _: u32, _: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(None)
    }

    fn output(&self, run: &str, port: &str, place: u32) -> Result<Option<Vec<u8>>, String> {
        let reader = Reader::open(&self.world.root, Arc::new(RealFs)).unwrap();
        let generation = reader.snapshot().unwrap();
        let name = format!("outputs/{port}/{place}");
        let entry = generation.run(run).and_then(|held| held.get(&name));
        Ok(entry.map(|entry| reader.read(entry).unwrap()))
    }
}

#[test]
fn a_fill_run_made_before_its_answer_was_kept_is_unanswerable_naming_it() {
    use whizbang_core::capability::recorded::{Recorded, StopKind};
    use whizbang_core::capability::{FillRequest, Picture, Support};
    use whizbang_core::contract::Cancellation;
    let world = world("replay-before", 1);
    let removed = removal(&world);
    let runs = family(&world, removed.run().as_str());
    let hiding = Hiding {
        world: &world,
        hidden: "diagnostics/fill-answer.png".to_owned(),
    };
    let recorded = Arc::new(Recorded::of_family(&runs, &hiding));
    let offered = recorded.offered();
    let fill = offered.made(Capability::Fill, "recorded").unwrap();
    let whizbang_core::contract::Made::Fill(fill) = fill else {
        panic!("not a filler");
    };
    let pixels = whizbang_core::identity::CanonicalPixels::from_rgba(2, 2, vec![9; 16]).unwrap();
    let support = Support::from_plane(2, 2, vec![Support::PAINT; 4]).unwrap();
    let request = FillRequest::new(Picture::new(pixels).unwrap(), support, None).unwrap();
    let handle = whizbang_core::capability::FillHandle::new(fill, Cancellation::new());
    let fault = handle.fill(&request).unwrap_err();
    assert!(
        fault.sentence().contains("keeps no fill-answer.png"),
        "{}",
        fault.sentence()
    );
    let stop = recorded.course().stopped().unwrap();
    assert_eq!(stop.kind, StopKind::Unanswerable);
    assert!(
        stop.why
            .contains("made before a fill kept its painted answer"),
        "{}",
        stop.why
    );
}

/// The build's actions, with `grow` changed: it hands back the mask it was given.
fn with_a_changed_grow() -> whizbang_core::contract::Registry {
    struct Unchanged(Arc<dyn Action>);
    impl Action for Unchanged {
        fn descriptor(&self) -> &ActionDescriptor {
            self.0.descriptor()
        }
        fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
            // The same pixels, under one more step of its origin: another identity, and
            // the same area for the filler to paint.
            let grown = self.0.run(call, context)?;
            let (_, mask) = grown.outputs().next().unwrap();
            let mut mask = mask.clone().into_mask().unwrap();
            let step = whizbang_core::artifact::Step::new("changed", "1.0.0");
            mask.record(step).unwrap();
            Ok(Produced::nothing().with("mask", mask))
        }
    }
    let built = wiring::registry().unwrap();
    let mut registry = whizbang_core::contract::Registry::new();
    let names: Vec<String> = built.descriptors().map(|d| d.name().to_owned()).collect();
    for name in names {
        let action = built.get(&name).unwrap().clone();
        let action: Arc<dyn Action> = match name.as_str() {
            "grow" => Arc::new(Unchanged(action)),
            _ => action,
        };
        registry.register(action).unwrap();
    }
    registry
}

#[test]
fn a_changed_operation_is_reported_as_a_difference_at_its_step() {
    let world = world("replay-changed", 1);
    let removed = removal(&world);
    let engine = world.engine_of(with_a_changed_grow());
    let (run, _) = replayed(&engine, &of_run(removed.run()));
    let report = reported(&world, &run);
    let whizbang_core::contract::replay::Report::Differs { differences, .. } = &report.report
    else {
        panic!("{}", report.text());
    };
    // The first difference is the grow's own output: the third step of the recipe.
    let grow = (record(&world, removed.run().as_str()).steps().iter())
        .position(|step| step.action() == "grow")
        .unwrap();
    assert_eq!(differences[0].place, format!("the run, step {}", grow + 1));
    assert_eq!(differences[0].what, "outputs");
    assert!(report.text().contains("report: differs"));
}

#[test]
fn a_child_an_interrupted_run_a_recipe_files_run_and_a_setting_at_its_limit_are_each_refused() {
    let world = world("replay-refused", 2);
    let removed = removal(&world);
    let engine = with_asking(&world);
    let refused = |replay: &Replay, engine: &Engine| -> String {
        match engine.start_replay(replay).unwrap_err() {
            CallError::Refused(Refusal::NotReplayed { why, .. }) => why,
            other => panic!("{other:?}"),
        }
    };
    // A child, naming the run to replay.
    let child = record(&world, removed.run().as_str()).steps()[0]
        .run()
        .to_owned();
    let why = refused(&of_run(&RunId::parse(&child).unwrap()), &engine);
    assert!(
        why.contains("replayed as part of the run it is under"),
        "{why}"
    );
    assert!(why.contains(removed.run().as_str()), "{why}");
    // An action this build does not ship: `asking` is the test's, as a recipe file's is.
    let asked = engine.submit(&Call::new("asking")).unwrap();
    drop(engine);
    let unshipped = world.engine_of({
        let mut registry = wiring::registry_over(&world.over, false).unwrap();
        registry.register(asking()).unwrap();
        registry
    });
    let why = refused(&of_run(asked.run()), &unshipped);
    assert!(
        why.contains("asking is not an action this build ships"),
        "{why}"
    );
    drop(unshipped);
    // An interrupted family, and a setting at its limit, from bundles of the removal.
    let made = whizbang::bundling::make(&world.root, &[removed.run().as_str()]).unwrap();
    let members = crate::bundle::Members::of(&made.bytes);
    let fill = members.run_of(0, "fill");
    let mut interrupted = members.clone();
    let fill_record = RunRecord::from_bytes(members.record(&fill).as_bytes()).unwrap();
    let root_id = removed.run().clone();
    let left = record::Left {
        run: &RunId::parse(&fill).unwrap(),
        whizbang: env!("CARGO_PKG_VERSION"),
        admitted: Some(record::Admitted {
            request: None,
            parent: Some(&root_id),
            action: "fill",
            version: fill_record.asked().version.as_deref().unwrap(),
            reserved_micros: None,
            start_ms: fill_record.time().start_ms.unwrap(),
        }),
        end_ms: fill_record.time().end_ms,
    };
    interrupted.strip(&fill);
    interrupted.members.insert(
        format!("runs/{fill}/record"),
        RunRecord::interrupted(left).unwrap().to_bytes(),
    );
    let noted = format!("\"run\":\"{fill}\",\"action\":\"fill\",\"status\":\"completed\"");
    let unknown = format!("\"run\":\"{fill}\",\"action\":\"fill\",\"status\":null");
    interrupted.edit_record(removed.run().as_str(), &noted, &unknown);
    let other = world_store("replay-refused-elsewhere");
    let digest = import(&other, &interrupted.bytes());
    let engine = bare_at(&other.1);
    let why = refused(&Replay::of(Source::Bundle { digest, step: 0 }), &engine);
    assert!(
        why.contains(&format!("the run {fill} of its family was interrupted")),
        "{why}"
    );
    drop(engine);
    // A setting at its limit: a setting's text is cut there with no mark.
    let mut long = members.clone();
    let segment = members.run_of(0, "segment");
    let text = "x".repeat(whizbang_core::contract::record::SETTING_TEXT_LIMIT);
    let record_text = long.record(&segment);
    let settings_at = record_text.find("\"settings\":[").unwrap() + "\"settings\":[".len();
    let mut edited = record_text.clone();
    let closing = if record_text[settings_at..].starts_with(']') {
        ""
    } else {
        ","
    };
    edited.insert_str(
        settings_at,
        &format!("{{\"name\":\"a.long\",\"value\":{{\"text\":\"{text}\"}}}}{closing}"),
    );
    long.members
        .insert(format!("runs/{segment}/record"), edited.into_bytes());
    if let Ok(checked) = whizbang_core::bundle::check(std::io::Cursor::new(long.bytes())) {
        let digest = import(&other, &long.bytes());
        let engine = bare_at(&other.1);
        assert_eq!(digest, checked.digest);
        let why = refused(&Replay::of(Source::Bundle { digest, step: 0 }), &engine);
        assert!(why.contains("a.long"), "{why}");
        assert!(why.contains("may have been cut"), "{why}");
    } else {
        // A record whose settings are out of order is not one; the core's rule is held
        // by the reading of the store's own runs alone.
        panic!("the edited record is not read");
    }
}

/// A store of its own on the real disk, in a folder removed with it.
fn world_store(label: &str) -> (whizbang_testkit::scratch::Scratch, std::path::PathBuf) {
    let dir = whizbang_testkit::scratch::Scratch::new(label);
    let root = dir.path().join("store");
    drop(Store::open(&root, Arc::new(RealFs)).unwrap());
    (dir, root)
}

/// A runner over a store under no config.
fn bare_at(root: &std::path::Path) -> Engine {
    let store = Arc::new(Store::open(root, Arc::new(RealFs)).unwrap());
    wiring::runner(wiring::registry().unwrap(), store, &Over::nothing()).unwrap()
}

/// Imports a bundle's bytes into a store: its digest.
fn import(
    store: &(whizbang_testkit::scratch::Scratch, std::path::PathBuf),
    bytes: &[u8],
) -> whizbang_core::identity::Digest {
    let file = store.0.path().join(format!(
        "{}.tar",
        whizbang_core::identity::Digest::of_bytes(bytes).hex()
    ));
    std::fs::write(&file, bytes).unwrap();
    let opened = Store::open(&store.1, Arc::new(RealFs)).unwrap();
    let imported = whizbang::bundling::import_into(&opened, &file).unwrap();
    imported.digest
}

#[test]
fn a_manifest_that_claims_a_step_replayable_that_is_not_changes_nothing() {
    // The writer marks a step whose root was interrupted not replayable; a manifest
    // edited to claim it is passes the reader, which checks the manifest's spelling and
    // not its judgment. Replay decides from the records, and so does `bundle show`.
    let world = world("replay-claim", 1);
    let removed = removal(&world);
    let made = whizbang::bundling::make(&world.root, &[removed.run().as_str()]).unwrap();
    let mut lost = crate::bundle::Members::of(&made.bytes);
    let root = removed.run().as_str().to_owned();
    let root_record = RunRecord::from_bytes(lost.record(&root).as_bytes()).unwrap();
    let left = record::Left {
        run: removed.run(),
        whizbang: env!("CARGO_PKG_VERSION"),
        admitted: Some(record::Admitted {
            request: None,
            parent: None,
            action: whizbang_rascal::checked::REMOVE,
            version: root_record.asked().version.as_deref().unwrap(),
            reserved_micros: None,
            start_ms: root_record.time().start_ms.unwrap(),
        }),
        end_ms: root_record.time().end_ms,
    };
    lost.strip(&root);
    lost.members.insert(
        format!("runs/{root}/record"),
        RunRecord::interrupted(left).unwrap().to_bytes(),
    );
    lost.manifest["steps"][0]["inputs"] = serde_json::json!([]);
    lost.manifest["steps"][0]["config"] = serde_json::json!(null);
    lost.manifest["steps"][0]["replayable"] = serde_json::json!(true);
    lost.manifest["steps"][0]["why"] = serde_json::json!(null);
    let other = world_store("replay-claim-elsewhere");
    let digest = import(&other, &lost.bytes());
    let engine = bare_at(&other.1);
    match engine.start_replay(&Replay::of(Source::Bundle { digest, step: 0 })) {
        Err(CallError::Refused(Refusal::NotReplayed { why, .. })) => {
            assert!(why.contains("was interrupted"), "{why}");
        }
        other => panic!("{other:?}"),
    }
    drop(engine);
    let file = other.0.path().join("claimed.tar");
    std::fs::write(&file, lost.bytes()).unwrap();
    let shown = whizbang(&["bundle", "show", path(&file)]);
    assert_eq!(shown.code, Some(0), "{}", shown.err);
    assert!(
        shown.out.contains("not replayable: the run ")
            && shown.out.contains("of its family was interrupted")
            && shown.out.contains("(its manifest says replayable)"),
        "{}",
        shown.out
    );
}

#[test]
fn a_replay_from_a_bundle_holds_its_inputs_after_the_bundle_is_deleted_and_pruned() {
    let chain = crate::bundle::chain("replay-held-inputs");
    let made = crate::bundle::made(&chain);
    let other = world_store("replay-held-inputs-elsewhere");
    let digest = import(&other, &made.bytes);
    let engine = bare_at(&other.1);
    let (check, _) = {
        let replay = Replay::of(Source::Bundle { digest, step: 1 });
        let Answer::Started(run) = engine.start_replay(&replay).unwrap() else {
            panic!("not started");
        };
        let waited = engine.wait(&run).unwrap();
        (run, waited)
    };
    drop(engine);
    let report = whizbang::replaying::report_of(&other.1, check.as_str()).unwrap();
    assert!(report.reproduced(), "{}", report.text());
    // The bundle goes, and what it alone named with it.
    let gone = whizbang(&[
        "bundle",
        "delete",
        &digest.to_string(),
        "--store",
        path(&other.1),
    ]);
    assert_eq!(gone.code, Some(0), "{}", gone.err);
    // The replay's inputs are its own: each is still read by its identity.
    let reader = Reader::open(&other.1, Arc::new(RealFs)).unwrap();
    let generation = reader.snapshot().unwrap();
    let made = record::stored(&generation, check.as_str(), |e| reader.read(e))
        .unwrap()
        .unwrap();
    assert!(!made.inputs().is_empty());
    for input in made.inputs() {
        let id = whizbang_core::artifact::ArtifactId(input.artifact);
        let read = whizbang_core::artifact::get_anywhere(
            &reader,
            input.artifact_type,
            id,
            &whizbang_core::identity::DecodeLimits::default(),
        );
        assert!(read.is_ok(), "{}: {:?}", input.port, read.err());
    }
}

#[test]
fn a_live_replay_spends_as_a_run_and_reports_a_changed_verdict_as_a_difference() {
    let world = Looped::new(
        "replay-live",
        &Setup::answering(vec![passes(), crate::checked::remains()]).on_disk(),
    );
    let removed = removal(&world);
    let checked = checking(&world, output(&removed, "image"));
    let sent = world.kit.requests();
    let (run, _) = replayed(&world.engine(), &of_run(checked.run()).live());
    assert!(world.kit.requests().2 > sent.2, "the judge was asked again");
    let report = reported(&world, &run);
    assert_eq!(report.report.name(), "differs", "{}", report.text());
    let text = report.text();
    let whizbang_core::contract::replay::Report::Differs { differences, .. } = &report.report
    else {
        unreachable!()
    };
    // The verdict differs, where the checklist and the decision are kept, and the call
    // is the original's: no argument differs.
    assert!(
        differences
            .iter()
            .any(|d| d.what == "judge" && d.original.contains("pass") && d.replay.contains("fail")),
        "{text}"
    );
    assert!(
        !differences
            .iter()
            .any(|d| d.what == "arguments" || d.what == "steps"),
        "{text}"
    );
    // What it spent is in its records, as any run's.
    let spent: usize = (family(&world, run.as_str()).iter())
        .map(|r| r.spend().requests.len())
        .sum();
    assert!(spent > 0);
    assert_eq!(
        record(&world, run.as_str()).replayed().map(|r| r.mode),
        Some(ReplayMode::Live)
    );
}

#[test]
fn a_live_replay_whose_backend_the_config_no_longer_offers_is_refused() {
    let world = world("replay-gone", 1);
    let removed = removal(&world);
    let checked = checking(&world, output(&removed, "image"));
    // Under no config, the judge the check ran under is no longer offered.
    let refused = bare(&world)
        .start_replay(&of_run(checked.run()).live())
        .unwrap_err();
    assert!(matches!(refused, CallError::Refused(_)), "{refused:?}");
    let said = refused.to_string();
    assert!(said.contains("judge"), "{said}");
}

#[test]
fn every_run_of_a_replayed_family_names_it_and_stats_leaves_the_family_out_by_default() {
    let world = world("replay-stats", 1);
    let removed = removal(&world);
    let runs_of = |args: &[&str]| -> usize {
        let mut all = vec!["runs", "stats", "--store", path(&world.root)];
        all.extend_from_slice(args);
        let said = whizbang(&all);
        assert_eq!(said.code, Some(0), "{}", said.err);
        let line = said.out.lines().find(|l| l.starts_with("runs: ")).unwrap();
        line.trim_start_matches("runs: ").parse().unwrap()
    };
    let ordinary = runs_of(&[]);
    let (run, _) = replayed(&bare(&world), &of_run(removed.run()));
    for made in family(&world, run.as_str()) {
        assert_eq!(made.replayed(), Some(&offline_of(removed.run())));
    }
    assert_eq!(
        runs_of(&[]),
        ordinary,
        "the figures of ordinary use are unchanged"
    );
    assert_eq!(runs_of(&["--replayed", "include"]), 2 * ordinary);
    assert_eq!(runs_of(&["--replayed", "only"]), ordinary);
    // A replayed run a dead process left: its record is the one a dead process leaves,
    // and its admission keeps what it replayed.
    let admitted = {
        let reader = Reader::open(&world.root, Arc::new(RealFs)).unwrap();
        let generation = reader.snapshot().unwrap();
        let entry = generation
            .run(run.as_str())
            .unwrap()
            .get(ADMITTED_NAME)
            .unwrap();
        String::from_utf8(reader.read(entry).unwrap()).unwrap()
    };
    let dead = RunId::parse("1000-424242").unwrap();
    let start = admission(&world, run.as_str()).start_ms().to_string();
    let bytes = admitted
        .replace(run.as_str(), dead.as_str())
        .replace(&format!("\"start_ms\":{start}"), "\"start_ms\":1000");
    assert!(Admission::from_bytes(bytes.as_bytes()).is_ok());
    {
        let store = Store::open(&world.root, Arc::new(RealFs)).unwrap();
        let mut publication = store.begin().unwrap();
        publication.create_run(dead.as_str()).unwrap();
        publication
            .put_in(
                dead.as_str(),
                ADMITTED_NAME,
                record::RECORD_KIND,
                bytes.as_bytes(),
            )
            .unwrap();
        publication.publish().unwrap();
    }
    // The next writer ends it as interrupted, and stats still leaves it out.
    let opening = wiring::opening(&world.root, wiring::Make::Nothing, wiring::recovery()).unwrap();
    drop(opening.writer().unwrap());
    drop(opening);
    let ended = record(&world, dead.as_str());
    assert_eq!(
        ended.outcome().error.as_ref().unwrap().kind,
        record::INTERRUPTED
    );
    assert_eq!(ended.replayed(), None);
    assert_eq!(runs_of(&[]), ordinary);
    assert_eq!(runs_of(&["--replayed", "only"]), ordinary + 1);
}

#[test]
fn a_repeat_by_bundle_digest_after_the_archive_and_the_entry_are_gone_publishes_nothing() {
    let chain = crate::bundle::chain("replay-repeat-bundle");
    let made = crate::bundle::made(&chain);
    let other = world_store("replay-repeat-bundle-elsewhere");
    let file = other.0.path().join("chain.tar");
    std::fs::write(&file, &made.bytes).unwrap();
    let store = path(&other.1).to_owned();
    let first = whizbang(&[
        "runs",
        "replay",
        "--bundle",
        path(&file),
        "--step",
        "1",
        "--request-id",
        "again",
        "--store",
        &store,
    ]);
    assert_eq!(first.code, Some(0), "{}{}", first.out, first.err);
    let replay = first.out.lines().next().unwrap().to_owned();
    // The file and the bundle's entry go.
    std::fs::remove_file(&file).unwrap();
    let gone = whizbang(&[
        "bundle",
        "delete",
        &made.digest.to_string(),
        "--store",
        &store,
    ]);
    assert_eq!(gone.code, Some(0), "{}", gone.err);
    let generation = || {
        Reader::open(&other.1, Arc::new(RealFs))
            .unwrap()
            .snapshot()
            .unwrap()
            .number()
    };
    let before = generation();
    let again = whizbang(&[
        "runs",
        "replay",
        "--bundle-digest",
        &made.digest.to_string(),
        "--step",
        "1",
        "--request-id",
        "again",
        "--store",
        &store,
    ]);
    assert_eq!(again.code, Some(0), "{}{}", again.out, again.err);
    assert_eq!(again.out.lines().next().unwrap(), replay);
    assert_eq!(generation(), before, "a repeat publishes nothing");
    // Another step under the id is a conflict.
    let other_step = whizbang(&[
        "runs",
        "replay",
        "--bundle-digest",
        &made.digest.to_string(),
        "--step",
        "0",
        "--request-id",
        "again",
        "--store",
        &store,
    ]);
    assert_eq!(other_step.code, Some(1));
    assert!(
        other_step.err.contains("used for another request"),
        "{}",
        other_step.err
    );
}

#[test]
fn a_turn_that_held_an_opaque_part_is_unanswerable_naming_it() {
    use whizbang_core::capability::{Declaration, Opaque, Roster, StopReason, Turn, Usage};
    use whizbang_core::contract::Capabilities;
    use whizbang_core::identity::Digest;
    use whizbang_testkit::capabilities::FakeJudge;
    // A judge whose turn holds a provider's opaque part: the store keeps its digest and
    // length, never its body, so a replay cannot give that turn back.
    let dir = whizbang_testkit::scratch::Scratch::new("replay-opaque");
    let store = Arc::new(Store::open(&dir.path().join("store"), Arc::new(RealFs)).unwrap());
    let model = whizbang_core::contract::ModelIdentity::new("fake", "judge", "1");
    let declaration = Declaration::new(model, &["image", "tool"]).unwrap();
    let opaque = Part::Opaque(Opaque::new("fake", "a signature only its provider reads").unwrap());
    let turn = Turn::new(
        vec![Part::Text("seen".to_owned()), opaque],
        StopReason::EndTurn,
        Some(Usage::new(1, 1)),
        None,
    )
    .unwrap();
    let judge = Arc::new(FakeJudge::scripted(vec![Ok(turn)]).declared_as(declaration.clone()));
    let roster = Roster::new()
        .offer(Capability::Judge, "fake", declaration)
        .and_then(|roster| roster.default_for(Capability::Judge, "fake"))
        .unwrap();
    let mut registry = whizbang_core::contract::Registry::new();
    registry.register(asking()).unwrap();
    let mut shipped = std::collections::BTreeMap::new();
    shipped.insert("asking".to_owned(), "1.0.0".to_owned());
    let engine = Engine::over(registry, Arc::new(roster), store, Digest::of_bytes(b""))
        .with_capabilities(Capabilities::none().with_judge(judge))
        .with_shipped(shipped);
    *ASKED.lock().unwrap() = 0;
    let original = engine.submit(&Call::new("asking")).unwrap();
    assert_eq!(
        original.outcome().name(),
        "completed",
        "{:?}",
        original.outcome()
    );
    *ASKED.lock().unwrap() = 0;
    let (run, _) = replayed(&engine, &of_run(original.run()));
    drop(engine);
    let reported = whizbang::replaying::report_of(&dir.path().join("store"), run.as_str()).unwrap();
    assert_eq!(
        reported.report.name(),
        "unanswerable",
        "{}",
        reported.text()
    );
    let text = reported.text();
    assert!(text.contains("opaque part of fake"), "{text}");
    assert!(
        text.contains(&format!(
            "{} bytes",
            "a signature only its provider reads".len()
        )),
        "{text}"
    );
}

#[test]
fn choices_are_found_by_alias_for_an_explicit_choice_a_default_and_two_aliases_of_one_model() {
    // Two judges are offered, by two names: the default, and one a call chooses. A check
    // that chose by name and one that took the default each replay offline, answered by
    // the model the settings name under the choice it made.
    let setup = Setup {
        dear: true,
        ..Setup::answering(vec![passes(); 2])
    };
    let world = Looped::new("replay-choices", &setup.on_disk());
    let removed = removal(&world);
    let image = output(&removed, "image");
    let by_default = checking(&world, image);
    let chosen = world.run(
        &Call::new(whizbang_rascal::checked::CHECK)
            .argument(
                "job",
                whizbang_core::contract::Value::text(&whizbang_rascal::checked::job(
                    crate::checked::PHRASE,
                )),
            )
            .argument(
                "judge",
                whizbang_core::contract::Value::text(crate::checked::DEAR),
            )
            .input("images", image)
            .input("images", world.image_id()),
    );
    assert_eq!(
        chosen.outcome().name(),
        "completed",
        "{:?}",
        chosen.outcome()
    );
    for original in [&by_default, &chosen] {
        let (run, _) = replayed(&bare(&world), &of_run(original.run()));
        let report = reported(&world, &run);
        assert!(report.reproduced(), "{}", report.text());
        let models = |root: &str| -> Vec<String> {
            (family(&world, root).iter())
                .flat_map(|r| {
                    r.models()
                        .iter()
                        .map(|m| m.model.clone())
                        .collect::<Vec<_>>()
                })
                .collect()
        };
        assert_eq!(models(run.as_str()), models(original.run().as_str()));
    }
}

#[test]
fn an_offline_replay_of_a_recipe_whose_check_takes_measurements_reproduces_it() {
    // `source-fidelity` measures first (`details`), and its check is handed those
    // measurements: the replay measures again, the measurements kept as `details`'s
    // diagnostic are compared byte for byte, and the check is answered as it was.
    let world = world("replay-measured", 1);
    let removed = removal(&world);
    let job = whizbang_rascal::checked::job(crate::checked::PHRASE);
    let checked = world.run(
        &Call::new(whizbang_rascal::checked::CHECK)
            .argument("job", whizbang_core::contract::Value::text(&job))
            // Measured first, whole: the measurements are the check's to read.
            .argument(
                "set",
                whizbang_core::contract::Value::text("details.details=first"),
            )
            .input("images", output(&removed, "image"))
            .input("images", world.image_id()),
    );
    assert_eq!(
        checked.outcome().name(),
        "completed",
        "{:?}",
        checked.outcome()
    );
    let measured = |root: &str| -> Vec<u8> {
        let family = family(&world, root);
        let details = family
            .iter()
            .find(|r| r.asked().action.as_deref() == Some(whizbang_squint::measured::NAME))
            .expect("the recipe measures first");
        let reader = Reader::open(&world.root, Arc::new(RealFs)).unwrap();
        let generation = reader.snapshot().unwrap();
        let name = format!("diagnostics/{}", whizbang_squint::measured::KEPT);
        let held = generation.run(&details.identity().run).unwrap();
        reader
            .read(held.get(&name).expect("the measurements are kept"))
            .unwrap()
    };
    let (run, _) = replayed(&bare(&world), &of_run(checked.run()));
    let report = reported(&world, &run);
    assert!(report.reproduced(), "{}", report.text());
    // Something was measured, and the replay measured the same: the comparison read the
    // two documents through the run match, each run's id as its place.
    let entries = |bytes: Vec<u8>| -> serde_json::Value {
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["entries"].clone()
    };
    let was = entries(measured(checked.run().as_str()));
    assert!(!was.as_array().unwrap().is_empty(), "{was}");
    assert_eq!(
        entries(measured(run.as_str())).as_array().unwrap().len(),
        was.as_array().unwrap().len()
    );
}

#[test]
fn a_changed_measurement_of_an_action_with_no_artifact_output_is_a_difference() {
    use whizbang_squint::compare::{EDGE_SIMILARITY, EDGE_SIMILARITY_REPORT};
    // `edge_similarity` commits no artifact: its result is its measurements, which it
    // keeps as its diagnostic. A build whose measurement differs is a difference there.
    struct Halved(Arc<dyn Action>);
    impl Action for Halved {
        fn descriptor(&self) -> &ActionDescriptor {
            self.0.descriptor()
        }
        fn run(&self, _: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
            let said = serde_json::json!({"edge_similarity": 0.5});
            (context.diagnostics())
                .write(EDGE_SIMILARITY_REPORT, said.to_string().as_bytes())
                .map_err(|why| Failure::error("diagnostic", &why))?;
            Ok(Produced::nothing().with_measurements(said))
        }
    }
    let world = world("replay-measurement", 1);
    let removed = removal(&world);
    let call = Call::new(EDGE_SIMILARITY)
        .input("frame", output(&removed, "image"))
        .input("against", world.image_id());
    let original = world.run(&call);
    assert_eq!(
        original.outcome().name(),
        "completed",
        "{:?}",
        original.outcome()
    );
    assert!(original.outcome().outputs().is_empty());
    let built = wiring::registry().unwrap();
    let mut registry = whizbang_core::contract::Registry::new();
    let names: Vec<String> = built.descriptors().map(|d| d.name().to_owned()).collect();
    for name in names {
        let action = built.get(&name).unwrap().clone();
        let action: Arc<dyn Action> = match name.as_str() {
            EDGE_SIMILARITY => Arc::new(Halved(action)),
            _ => action,
        };
        registry.register(action).unwrap();
    }
    let (run, _) = replayed(&world.engine_of(registry), &of_run(original.run()));
    let report = reported(&world, &run);
    let whizbang_core::contract::replay::Report::Differs { differences, .. } = &report.report
    else {
        panic!("{}", report.text());
    };
    assert_eq!(differences[0].place, "the run");
    assert_eq!(
        differences[0].what,
        format!("diagnostic diagnostics/{EDGE_SIMILARITY_REPORT}")
    );
}

#[test]
fn a_divergence_in_a_second_look_or_a_loops_check_is_the_replays_report_whatever_the_action_made_of_it()
 {
    // The loop's check, changed, sends its judge another request than the original's:
    // diverged there. The loop turns the failed check into an unchecked attempt and goes
    // on; the replay's report is the divergence all the same.
    let world = world("replay-loop-diverged", 2);
    let original = world.run(&world.call());
    assert_eq!(
        original.outcome().name(),
        "completed",
        "{:?}",
        original.outcome()
    );
    let sent = world.kit.requests();
    let registry = changing("check", |inner| {
        Arc::new(Asks(
            inner,
            Box::new(|_, context| {
                let judge = context.capabilities().judge().expect("a judge is handed");
                let text = "another question".to_owned();
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
            }),
        ))
    });
    let (run, outcome) = replayed(&world.engine_of(registry), &of_run(original.run()));
    assert_eq!(world.kit.requests(), sent, "nothing was called");
    let report = reported(&world, &run);
    assert_eq!(
        report.report.name(),
        "diverged",
        "{}\n{outcome:?}",
        report.text()
    );
    let whizbang_core::contract::replay::Report::Stopped(stop) = &report.report else {
        unreachable!()
    };
    assert_eq!(stop.capability, Capability::Judge);
    // Whatever the loop made of it, as the record says: not the report.
    let made = record(&world, run.as_str());
    assert!(made.attempts().is_some());
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
struct Asks(
    Arc<dyn Action>,
    Box<dyn Fn(&PreparedCall, &Context) -> Result<Produced, Failure> + Send + Sync>,
);

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

#[test]
fn a_live_replay_with_the_same_answers_reproduces_a_check_and_a_bundles_step() {
    // Under the config the runs ran under, every backend a fake that answers as it did:
    // a live replay makes the same calls, the choices the config makes by default not
    // written into them, and is reproduced; through the command it exits 0.
    let world = world("replay-live-same", 3);
    let removed = removal(&world);
    let checked = checking(&world, output(&removed, "image"));
    let (run, _) = replayed(&world.engine(), &of_run(checked.run()).live());
    let report = reported(&world, &run);
    assert!(report.reproduced(), "{}", report.text());
    // A step of a bundle of the removal, taken into the same store.
    let made = whizbang::bundling::make(&world.root, &[removed.run().as_str()]).unwrap();
    let dir = whizbang_testkit::scratch::Scratch::new("replay-live-same-file");
    let file = dir.path().join("removal.tar");
    std::fs::write(&file, &made.bytes).unwrap();
    {
        let store = Store::open(&world.root, Arc::new(RealFs)).unwrap();
        whizbang::bundling::import_into(&store, &file).unwrap();
    }
    let replay = Replay::of(Source::Bundle {
        digest: made.digest,
        step: 0,
    })
    .live();
    let (run, _) = replayed(&world.engine(), &replay);
    let report = reported(&world, &run);
    assert!(report.reproduced(), "{}", report.text());
    let offline = whizbang(&[
        "runs",
        "replay",
        "--bundle-digest",
        &made.digest.to_string(),
        "--step",
        "0",
        "--store",
        path(&world.root),
    ]);
    assert_eq!(offline.code, Some(0), "{}{}", offline.out, offline.err);
}
