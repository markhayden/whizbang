//! The interrupted record (ADR 0015; plan 1B, task I4b): the part of a store's
//! recovery that ends the runs a dead process left, each with a record made from its
//! admission alone, and what is then answered of such a run.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::Value as Json;
use whizbang_core::capability::{FillRequest, Picture, Support};
use whizbang_core::contract::record::{
    Admitted, INTERRUPTED, Left, Lost, RunRecord, Status, StoredRecordError,
};
use whizbang_core::contract::request::{self, Admission, Ending};
use whizbang_core::contract::{
    ADMITTED_NAME, Action, Answer, Backends, Call, CallError, Capabilities, Capability, Clock,
    Engine, Estimate, Executor, Interrupted, ModelIdentity, Opening, Place, Produced, Recovery,
    RecoveryPart, Registry, Request, RequestId, Requirement, RunId, RunState, Unavailable,
    WHIZBANG_VERSION, record,
};
use whizbang_core::identity::{CanonicalPixels, Digest};
use whizbang_core::store::{self, Reader, Store};
use whizbang_testkit::around::FakeWrapper;
use whizbang_testkit::barrier::Barriers;
use whizbang_testkit::capabilities::{FakeFiller, Paint};
use whizbang_testkit::fs_shim::{Event, Lost as Gone, ShimFs};
use whizbang_testkit::ledger::FakeLedger;
use whizbang_testkit::schema::validate;

use super::asked::action;
use super::commit::World;
use super::record::schema;
use super::redact::made_up_key;

// --- what the tests are made of ----------------------------------------------------------------

struct Stopped(u64);

impl Clock for Stopped {
    fn now_ms(&self) -> u64 {
        self.0
    }
}

/// Backends with one model, whose every request may cost a quarter of a dollar.
struct Priced;

impl Backends for Priced {
    fn resolve(
        &self,
        _: Capability,
        _: Option<&str>,
        _: &[String],
    ) -> Result<ModelIdentity, Unavailable> {
        Ok(ModelIdentity::new("fake", "fake-filler", "1"))
    }

    fn bound(&self, _: Capability, _: &ModelIdentity) -> Estimate {
        Estimate::of_micros(250_000)
    }
}

/// An executor over this store, with a ledger, whose clock says 7000.
pub(super) fn engine(store: Arc<Store>, actions: Vec<Arc<dyn Action>>) -> Engine {
    let mut registry = Registry::new();
    for action in actions {
        registry.register(action).unwrap();
    }
    Engine::over(registry, Arc::new(Priced), store, Digest::of_bytes(b""))
        .with_clock(Arc::new(Stopped(7000)))
        .with_ledger(Arc::new(FakeLedger::new()))
}

fn paid() -> Vec<Requirement> {
    vec![Requirement::new(Capability::Fill)]
}

/// An action that loses power while it runs: nothing of its run's end is written.
pub(super) fn dying(name: &str, world: &World) -> Arc<dyn Action> {
    let fs = world.fs.clone();
    action(name, paid(), move |_, _| {
        fs.crash(Gone::Unsynced);
        Ok(Produced::nothing())
    })
}

/// The recovery of this build's executor: the part that ends what was left, at 9000.
pub(super) fn recovery() -> Recovery {
    let part: Arc<dyn RecoveryPart> = Arc::new(Interrupted);
    Recovery::none()
        .with_part(part)
        .with_clock(Arc::new(Stopped(9000)))
}

/// Opens the world's store again, as the next process does, and recovers it.
pub(super) fn reopened(world: &World) -> (Opening, Arc<Store>, Arc<ShimFs>) {
    let fs = Arc::new(ShimFs::new());
    let opening = Opening::open(&world.root(), fs.clone(), recovery()).unwrap();
    let store = opening.writer().unwrap();
    (opening, store, fs)
}

fn id(text: &str) -> RequestId {
    RequestId::parse(text).unwrap()
}

/// What a store holds: its names outside runs with their content, and each run's.
type Names = BTreeMap<String, Vec<u8>>;

fn held(world: &World) -> (u64, Names, BTreeMap<String, Names>) {
    let reader = Reader::open(&world.root(), Arc::new(ShimFs::new())).unwrap();
    let generation = reader.snapshot().unwrap();
    let read = |entries: &BTreeMap<String, store::Entry>| -> Names {
        let read = |(name, entry): (&String, &store::Entry)| {
            // What cannot be read is held as nothing: a damaged entry.
            (name.clone(), reader.read(entry).unwrap_or_default())
        };
        entries.iter().map(read).collect()
    };
    let runs = generation.runs().iter();
    (
        generation.number(),
        read(generation.entries()),
        runs.map(|(id, run)| (id.clone(), read(run.entries())))
            .collect(),
    )
}

/// The run's record as the store holds it, held to the schema of `run/3`.
pub(super) fn record_of(world: &World, run: &str) -> Option<RunRecord> {
    let reader = Reader::open(&world.root(), Arc::new(ShimFs::new())).unwrap();
    let generation = reader.snapshot().unwrap();
    match record::stored(&generation, run, |entry| reader.read(entry))? {
        Ok(found) => {
            let document: Json = serde_json::from_slice(&found.to_bytes()).unwrap();
            validate(&schema(), &document).unwrap();
            Some(found)
        }
        Err(StoredRecordError::NoRecord) => None,
        Err(other) => panic!("{run}: {other}"),
    }
}

/// The record's error, and what it has lost.
fn ended_as(record: &RunRecord) -> (Status, &str, &[Lost]) {
    let outcome = record.outcome();
    let kind = outcome
        .error
        .as_ref()
        .map_or("", |error| error.kind.as_str());
    (outcome.status, kind, outcome.lost.as_slice())
}

fn interrupted() -> (Status, &'static str, &'static [Lost]) {
    (Status::Error, INTERRUPTED, Lost::but_the_admission())
}

/// The record recovery makes of a run from this admission, ended at 9000.
fn made_from(admission: &Admission) -> RunRecord {
    RunRecord::interrupted(Left {
        run: admission.run(),
        whizbang: WHIZBANG_VERSION,
        admitted: Some(Admitted {
            request: admission.request(),
            parent: admission.parent(),
            action: admission.action(),
            version: admission.version(),
            reserved_micros: admission.reserved_micros(),
            start_ms: admission.start_ms(),
        }),
        end_ms: 9000,
    })
    .unwrap()
}

/// Leaves one run of `paint`, asked under `order-17`, by a process that lost power
/// while the run's action ran. The run's id.
pub(super) fn leave_one(world: &World) -> String {
    let engine = engine(world.store(), vec![dying("paint", world)]);
    let asked = Request::new(Call::new("paint")).with_id(id("order-17"));
    let answered = engine.request(&asked);
    assert!(matches!(answered, Err(CallError::Store(_))), "{answered:?}");
    drop(engine);
    let (_, _, runs) = held(world);
    let [run] = runs.keys().cloned().collect::<Vec<_>>().try_into().unwrap();
    run
}

// --- a run a killed process left ---------------------------------------------------------------

#[test]
fn a_run_a_killed_process_left_is_ended_after_the_next_open_to_write_with_a_record_that_holds_exactly_the_admissions_facts_and_names_every_other_group_as_lost()
 {
    let world = World::new();
    let run = leave_one(&world);
    let (found, names, runs) = held(&world);
    // As it was left: admitted, with its reservation and its request's record, and
    // neither a record nor an ending.
    assert_eq!(runs[&run].keys().collect::<Vec<_>>(), [ADMITTED_NAME]);
    assert!(names.contains_key("requests/order-17/admitted"));
    assert!(!names.contains_key("requests/order-17/ended"));
    let admission = Admission::from_bytes(&runs[&run][ADMITTED_NAME]).unwrap();
    assert_eq!(admission.reserved_micros(), Some(250_000));

    let (_opening, _store, _) = reopened(&world);
    let (number, names_after, runs_after) = held(&world);
    // One publication: the record among the run's names, and the id's ending.
    assert_eq!(number, found + 1);
    let of_run: Vec<&String> = runs_after[&run].keys().collect();
    assert_eq!(of_run, [ADMITTED_NAME, "record"]);
    assert_eq!(runs_after[&run][ADMITTED_NAME], runs[&run][ADMITTED_NAME]);
    let record = record_of(&world, &run).unwrap();
    assert_eq!(ended_as(&record), interrupted());
    // Exactly the admission's facts, and the time and the version of the recovery:
    // the record is the one made from the admission alone, byte for byte.
    assert_eq!(runs_after[&run]["record"], made_from(&admission).to_bytes());
    let identity = record.identity();
    assert_eq!(identity.run, run);
    assert_eq!(identity.request.as_deref(), Some("order-17"));
    assert_eq!(
        (identity.parent.as_deref(), identity.whizbang.as_str()),
        (None, WHIZBANG_VERSION)
    );
    let asked = record.asked();
    assert_eq!(
        (asked.action.as_deref(), asked.version.as_deref()),
        (Some("paint"), Some("1.0.0"))
    );
    assert_eq!(record.spend().reserved_micros, Some(250_000));
    assert_eq!(
        (record.time().start_ms, record.time().end_ms),
        (Some(7000), 9000)
    );
    // Every other group is named as lost, and holds nothing.
    assert_eq!(record.outcome().lost.len(), 12);
    assert!(asked.arguments.is_empty() && asked.config.is_none());
    assert!(record.egress().is_empty() && record.models().is_empty());
    assert!(record.steps().is_empty() && record.diagnostics().is_empty());
    // For a request with an id, its ending: the run, how it ended, and when.
    let ending = Ending::from_bytes(&names_after["requests/order-17/ended"]).unwrap();
    assert_eq!(ending.run().as_str(), run);
    assert_eq!(
        (ending.request(), ending.status()),
        (&id("order-17"), Status::Error)
    );
    assert_eq!(ending.end_ms(), 9000);
    // Nothing else was made, and nothing of the money was touched: the reservation
    // is as it was, open, and counts in full until the ledger's own part closes it.
    let made: Vec<&String> = names_after
        .keys()
        .filter(|name| !names.contains_key(*name))
        .collect();
    assert_eq!(made, ["requests/order-17/ended"]);
    assert!(names_after.contains_key(&format!("spend/reservations/{run}")));
    assert!(
        !names_after
            .keys()
            .any(|name| name.starts_with("spend/closed/"))
    );
}

#[test]
fn a_child_and_its_parent_both_left_are_both_ended_each_from_its_own_admission() {
    let world = World::new();
    let outer = action("outer", paid(), |_, context| {
        context
            .executor()
            .submit(&Call::new("inner"))
            .map(drop)
            .ok();
        Ok(Produced::nothing())
    });
    let engine = engine(world.store(), vec![outer, dying("inner", &world)]);
    let answered = engine.request(&Request::new(Call::new("outer")));
    assert!(matches!(answered, Err(CallError::Store(_))), "{answered:?}");
    drop(engine);
    let (_, _, runs) = held(&world);
    assert_eq!(runs.len(), 2);
    let admissions: Vec<Admission> = runs
        .values()
        .map(|names| Admission::from_bytes(&names[ADMITTED_NAME]).unwrap())
        .collect();
    let parent = admissions.iter().find(|a| a.parent().is_none()).unwrap();
    let child = admissions.iter().find(|a| a.parent().is_some()).unwrap();
    assert_eq!(child.parent(), Some(parent.run()));

    let (_opening, _store, _) = reopened(&world);
    let (_, _, after) = held(&world);
    for admission in [parent, child] {
        let run = admission.run().as_str();
        assert_eq!(
            after[run]["record"],
            made_from(admission).to_bytes(),
            "{run}"
        );
        let record = record_of(&world, run).unwrap();
        assert_eq!(ended_as(&record), interrupted());
        assert_eq!(record.asked().action.as_deref(), Some(admission.action()));
    }
    let record = record_of(&world, child.run().as_str()).unwrap();
    assert_eq!(
        record.identity().parent.as_deref(),
        Some(parent.run().as_str())
    );
    assert_eq!(record.asked().action.as_deref(), Some("inner"));
    // The child drew on its parent: what it was given is in its record, from its
    // own admission, and the tree has one reservation, the root's.
    assert_eq!(record.spend().reserved_micros, child.reserved_micros());
}

#[test]
fn a_run_that_is_executing_in_this_process_and_one_this_process_abandoned_are_left_alone() {
    let world = World::new();
    let left = leave_one(&world);
    let (opening, store, fs) = reopened(&world);
    assert!(record_of(&world, &left).is_some());
    // This process then makes two runs of its own: one whose action is still
    // running, and one whose end the store refused, which it abandoned.
    let barriers = Barriers::new();
    let waits = {
        let barriers = barriers.clone();
        action("waits", paid(), move |_, _| {
            barriers.stop_at("running");
            Ok(Produced::nothing())
        })
    };
    let fails = {
        let fs = fs.clone();
        action("fails", paid(), move |_, _| {
            fs.refuse_matching(0, |event| matches!(event, Event::Write(_)));
            Ok(Produced::nothing())
        })
    };
    let engine = engine(store, vec![waits, fails]);
    let Ok(Answer::Started(executing)) = engine.start(&Request::new(Call::new("waits"))) else {
        panic!("the run was not started");
    };
    barriers.arrived("running", 1);
    let abandoned = engine.request(&Request::new(Call::new("fails")));
    assert!(
        matches!(abandoned, Err(CallError::Store(_))),
        "{abandoned:?}"
    );
    let (_, _, runs) = held(&world);
    let mine: Vec<&String> = runs.keys().filter(|run| **run != left).collect();
    assert_eq!(mine.len(), 2);
    let abandoned = RunId::parse(
        mine.iter()
            .find(|run| ***run != executing.as_str())
            .unwrap(),
    );
    let abandoned = abandoned.unwrap();
    let state = engine.status(&abandoned).unwrap().state().clone();
    assert!(matches!(state, RunState::Abandoned { .. }), "{state:?}");
    // Whoever asks this opening for its writer again is handed it: the store was
    // recovered once, before this process made a run, and is not recovered again.
    // Both runs are as they were: admitted, with no record.
    let before = held(&world);
    opening.writer().unwrap();
    opening.clone().remover().unwrap();
    assert_eq!(held(&world), before);
    for run in mine {
        assert!(record_of(&world, run).is_none(), "{run}");
    }
    // The one that executes goes on, and ends by itself, as completed.
    barriers.open("running");
    let state = engine.wait(&executing).unwrap().state().clone();
    assert!(matches!(state, RunState::Ended(_)), "{state:?}");
    let record = record_of(&world, executing.as_str()).unwrap();
    assert_eq!(
        (record.outcome().status, record.outcome().lost.len()),
        (Status::Completed, 0)
    );
    // The abandoned one stays so until this process is gone: the next open ends it.
    drop((engine, opening));
    let (_opening, _store, _) = reopened(&world);
    let record = record_of(&world, abandoned.as_str()).unwrap();
    assert_eq!(ended_as(&record), interrupted());
    let record = record_of(&world, executing.as_str()).unwrap();
    assert_eq!(record.outcome().status, Status::Completed);
}

#[test]
fn a_run_that_a_wrappers_failure_abandoned_is_ended_at_the_next_open_and_its_reservation_is_left_open()
 {
    // A meter that fails once its request's backend has answered: the run is
    // abandoned (ADR 0015, "A wrapper around each request"), and the money may be
    // spent.
    let world = World::new();
    let filler = Arc::new(FakeFiller::new(Paint::Inverse).charging(7));
    let meter = FakeWrapper::new("meter");
    meter.fail_after("its settlement was not published");
    let asks = action("asks", paid(), |_, context| {
        let rgba = (0..24u8).flat_map(|at| [at, at, at, 255]).collect();
        let picture = Picture::new(CanonicalPixels::from_rgba(6, 4, rgba).unwrap()).unwrap();
        let support = Support::from_plane(6, 4, vec![255; 24]).unwrap();
        let fill = context.capabilities().fill().unwrap();
        let _ = fill.fill(&FillRequest::new(picture, support, None).unwrap());
        Ok(Produced::nothing())
    });
    let engine = engine(world.store(), vec![asks])
        .with_capabilities(Capabilities::none().with_fill(filler.clone()))
        .with_wrapper(Place::Meter, Arc::new(meter));
    let asked = Request::new(Call::new("asks")).with_id(id("order-17"));
    let failed = engine.request(&asked).unwrap_err();
    let CallError::Abandoned { run, .. } = failed else {
        panic!("{failed:?}");
    };
    assert_eq!(filler.requests(), 1);
    let before = held(&world).1;
    assert!(record_of(&world, run.as_str()).is_none());
    // The process ends. The next open ends the run, as interrupted, with its
    // request's ending.
    drop(engine);
    let (_opening, store, _) = reopened(&world);
    let record = record_of(&world, run.as_str()).unwrap();
    assert_eq!(ended_as(&record), interrupted());
    assert_eq!(record.spend().reserved_micros, Some(250_000));
    let after = held(&world).1;
    assert!(after.contains_key("requests/order-17/ended"));
    // Its reservation is not closed by this part, and nothing of it is released: it
    // is open, and counts in full. Closing it, as held, is the ledger's own part of
    // the recovery (plan 1B, task S6).
    let reservation = format!("spend/reservations/{run}");
    assert_eq!(after[&reservation], before[&reservation]);
    assert!(!after.keys().any(|name| name.starts_with("spend/closed/")));
    let spend = |names: &Names| -> Vec<String> {
        let spend = names.keys().filter(|name| name.starts_with("spend/"));
        spend.cloned().collect()
    };
    assert_eq!(spend(&after), spend(&before));
    // The same request again is answered with the run that ended, and nothing runs.
    let again = self::engine(
        store,
        vec![action("asks", paid(), |_, _| panic!("it ran again"))],
    );
    let Ok(Answer::Repeat(earlier)) = again.request(&asked) else {
        panic!("not answered as a repeat");
    };
    assert_eq!(earlier.run(), &run);
    assert_eq!(filler.requests(), 1);
}

#[test]
fn an_admission_that_was_visible_and_not_confirmed_is_ended_at_the_next_open_and_its_request_is_then_answered_with_the_run_that_ended()
 {
    // The last sync of the admission's publication is refused: the run is in the
    // store, its action never starts, and in the process that made it the same
    // request is answered with a run that will not end there (ADR 0007).
    let world = World::new();
    let ran = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let paint = || {
        let ran = ran.clone();
        action("paint", paid(), move |_, _| {
            ran.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(Produced::nothing())
        })
    };
    let engine = engine(world.store(), vec![paint()]);
    let root = world.root();
    world.fs.refuse_matching(
        0,
        move |event| matches!(event, Event::SyncDir(dir) if *dir == root),
    );
    let asked = Request::new(Call::new("paint")).with_id(id("order-17"));
    let refused = engine.request(&asked).unwrap_err();
    assert!(
        refused.to_string().contains("not confirmed durable"),
        "{refused}"
    );
    let Ok(Answer::Repeat(earlier)) = engine.request(&asked) else {
        panic!("not answered as a repeat");
    };
    assert!(matches!(earlier.state(), RunState::Abandoned { .. }));
    let run = earlier.run().clone();
    drop(engine);
    // The next open confirms the generation it finds, and ends the run.
    let (_opening, store, fs) = reopened(&world);
    assert!(
        matches!(fs.events().iter().find(|event| matches!(event, Event::SyncDir(_))), Some(Event::SyncDir(dir)) if *dir == world.root())
    );
    let record = record_of(&world, run.as_str()).unwrap();
    assert_eq!(ended_as(&record), interrupted());
    // The same request is then answered with the run that ended, as an error of
    // the kind `interrupted`; its action never ran, and does not run now.
    let engine = self::engine(store, vec![paint()]);
    let Ok(Answer::Repeat(earlier)) = engine.request(&asked) else {
        panic!("not answered as a repeat");
    };
    assert_eq!(earlier.run(), &run);
    let RunState::Ended(whizbang_core::contract::Outcome::Error { kind, .. }) = earlier.state()
    else {
        panic!("{:?}", earlier.state());
    };
    assert_eq!(kind, INTERRUPTED);
    assert_eq!(ran.load(std::sync::atomic::Ordering::SeqCst), 0);
}

// --- what is left alone ------------------------------------------------------------------------

/// Plants a run with these names, as other hands would, and a name outside runs.
fn plant(world: &World, run: &str, names: &[(&str, &[u8])], outside: &[(&str, &[u8])]) {
    let store = world.store();
    let mut publication = store.begin().unwrap();
    publication.create_run(run).unwrap();
    for (name, bytes) in names {
        publication
            .put_in(run, name, store::Kind::Diagnostic, bytes)
            .unwrap();
    }
    for (name, bytes) in outside {
        publication
            .create(name, store::Kind::Diagnostic, bytes)
            .unwrap();
    }
    publication.publish().unwrap();
}

#[test]
fn a_run_that_ended_and_a_run_that_was_never_admitted_are_left_as_they_are_and_recovery_run_twice_changes_nothing()
 {
    let world = World::new();
    // A run that ended by itself, under an id.
    let ends = action("ends", paid(), |_, _| Ok(Produced::nothing()));
    let engine = engine(world.store(), vec![ends]);
    let asked = Request::new(Call::new("ends")).with_id(id("order-1"));
    let Ok(Answer::Ran(finished)) = engine.request(&asked) else {
        panic!("the request did not run");
    };
    drop(engine);
    // A run other hands made, with neither a record nor an admission; and one whose
    // id is no id of a run this build makes, with what is no admission.
    plant(&world, "other-hands", &[("notes", b"a note\n")], &[]);
    plant(
        &world,
        "not a run id",
        &[(ADMITTED_NAME, b"something\n")],
        &[],
    );
    let before = held(&world);
    let (opening, _, fs) = reopened(&world);
    assert_eq!(held(&world), before, "nothing was left to end");
    let published = |events: &[Event]| events.iter().any(|event| matches!(event, Event::Write(_)));
    assert!(!published(&fs.events()));
    let record = record_of(&world, finished.run().as_str()).unwrap();
    assert_eq!(record.outcome().status, Status::Completed);
    drop(opening);

    // One that was left is ended once: the open after the one that ended it finds
    // its record and its ending made, and publishes nothing.
    let left = World::new();
    let run = leave_one(&left);
    drop(reopened(&left));
    let ended = held(&left);
    let (_opening, _, fs) = reopened(&left);
    assert_eq!(held(&left), ended);
    assert!(!published(&fs.events()));
    assert!(record_of(&left, &run).is_some());
}

// --- a damaged admission -----------------------------------------------------------------------

#[test]
fn a_run_with_a_damaged_admission_is_ended_from_its_name_alone_and_names_the_admission_as_lost_and_a_marker_secret_in_it_is_in_nothing_the_recovery_wrote()
 {
    let marker = made_up_key("I4b");
    let garbage = format!("{{\"record\":\"admission/1\",\"key\":\"{marker}\"}}\n");
    // What an admission is not: bytes that are no admission, the admission of
    // another run, and one that is over the bound a record of a request has.
    let of_another = {
        let world = World::new();
        let run = leave_one(&world);
        held(&world).2[&run][ADMITTED_NAME].clone()
    };
    let large = vec![b' '; 5000];
    let cases: [(&str, &[u8]); 3] = [
        ("not one", garbage.as_bytes()),
        ("of another run", &of_another),
        ("too large", &large),
    ];
    for (what, admitted) in cases {
        let world = World::new();
        let run = "1000-1";
        plant(
            &world,
            run,
            &[(ADMITTED_NAME, admitted)],
            &[("requests/order-9/admitted", admitted)],
        );
        let before = held(&world);
        let (_opening, store, _) = reopened(&world);
        // Ended, with what its name gives: the run's id, and nothing of what an
        // admission says. Every group is lost, the admission among them.
        let record = record_of(&world, run).unwrap();
        assert_eq!(
            ended_as(&record),
            (Status::Error, INTERRUPTED, &Lost::ALL[..]),
            "{what}"
        );
        let identity = record.identity();
        assert_eq!(identity.run, run);
        assert_eq!(
            (identity.request.as_deref(), identity.parent.as_deref()),
            (None, None)
        );
        assert_eq!(
            (record.asked().action.as_deref(), record.time().start_ms),
            (None, None)
        );
        assert_eq!(
            (record.spend().reserved_micros, record.time().end_ms),
            (None, 9000)
        );
        // What the recovery wrote is the record, and nothing else: no ending is
        // made for an id that only the admission named. Nothing it wrote holds the
        // marker, or anything else of the admission.
        let after = held(&world);
        assert_eq!(after.1, before.1, "{what}: no name outside runs was made");
        let wrote = &after.2[run]["record"];
        assert!(!String::from_utf8_lossy(wrote).contains(&marker), "{what}");
        assert_eq!(wrote.len(), after.2[run]["record"].len());
        assert_eq!(after.2[run][ADMITTED_NAME], before.2[run][ADMITTED_NAME]);
        // Its request id stays taken. The same request under it, and a different
        // one, are both answered with the error of a record that is not read:
        // never as a repeat, and never as free. Nothing runs.
        let ran = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let counting = |name: &str| {
            let ran = ran.clone();
            action(name, paid(), move |_, _| {
                ran.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(Produced::nothing())
            })
        };
        let engine = engine(store, vec![counting("paint"), counting("other")]);
        for call in ["paint", "other"] {
            let asked = Request::new(Call::new(call)).with_id(id("order-9"));
            let refused = engine.request(&asked).unwrap_err();
            let CallError::Record { .. } = &refused else {
                panic!("{what}, {call}: {refused:?}");
            };
            assert!(
                !format!("{refused} {refused:?}").contains(&marker),
                "{what}"
            );
        }
        assert_eq!(ran.load(std::sync::atomic::Ordering::SeqCst), 0, "{what}");
        assert_eq!(held(&world), after, "{what}: nothing was admitted");
    }
    // And an admission that is whole is read with the same reader a look-up uses.
    let world = World::new();
    let run = leave_one(&world);
    let reader = Reader::open(&world.root(), Arc::new(ShimFs::new())).unwrap();
    let generation = reader.snapshot().unwrap();
    let read = request::admitted(&generation, &run, |entry| reader.read(entry));
    assert!(matches!(read, Some(Ok(_))));
}

#[test]
fn a_run_whose_admissions_content_is_gone_from_the_disk_is_ended_from_its_name_alone() {
    let world = World::new();
    let run = leave_one(&world);
    // The file that holds the admission's bytes is cut short, as damage does.
    let reader = Reader::open(&world.root(), Arc::new(ShimFs::new())).unwrap();
    let generation = reader.snapshot().unwrap();
    let entry = generation.run(&run).unwrap().get(ADMITTED_NAME).unwrap();
    let file = world.root().join("objects").join(entry.digest.hex());
    std::fs::write(&file, b"cut").unwrap();
    assert!(reader.read(entry).is_err());
    drop(reader);
    let (_opening, _store, _) = reopened(&world);
    let record = record_of(&world, &run).unwrap();
    assert_eq!(
        ended_as(&record),
        (Status::Error, INTERRUPTED, &Lost::ALL[..])
    );
    // No ending is made for the request: its record is the same damaged content.
    assert!(!held(&world).1.contains_key("requests/order-17/ended"));
}

// --- what readers say of an interrupted run -----------------------------------------------------

/// The state of a run that was ended as interrupted: an error of that kind.
fn says_interrupted(state: &RunState) {
    let RunState::Ended(whizbang_core::contract::Outcome::Error { kind, message }) = state else {
        panic!("{state:?}");
    };
    assert_eq!(kind, INTERRUPTED);
    assert_eq!(message, "the process that ran it ended before the run did");
}

#[test]
fn a_repeat_of_the_request_of_an_interrupted_run_is_ended_with_the_error_interrupted_and_status_and_wait_say_the_same_and_cancel_is_not_accepted()
 {
    use whizbang_core::contract::{Cancel, NoBackends, Refusal};
    let world = World::new();
    let run = leave_one(&world);
    let run = RunId::parse(&run).unwrap();
    let (opening, store, _) = reopened(&world);
    let ran = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let counting = |name: &str| {
        let ran = ran.clone();
        action(name, paid(), move |_, _| {
            ran.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(Produced::nothing())
        })
    };
    // By the executor that writes the store, and by one that only reads it.
    let writing = engine(store, vec![counting("paint"), counting("other")]);
    let mut registry = Registry::new();
    registry.register(counting("paint")).unwrap();
    let reader = Arc::new(opening.reader().unwrap());
    let reading = Engine::new(
        registry,
        Arc::new(NoBackends),
        reader,
        Digest::of_bytes(b""),
    );
    let asked = Request::new(Call::new("paint")).with_id(id("order-17"));
    for engine in [&writing, &reading] {
        // A repeat: the run that ended, and how. Nothing runs.
        for again in [engine.request(&asked), engine.start(&asked)] {
            let Ok(Answer::Repeat(earlier)) = again else {
                panic!("not answered as a repeat");
            };
            assert_eq!((earlier.run(), earlier.start_ms()), (&run, 7000));
            says_interrupted(earlier.state());
        }
        // `status` says the same, and so does `wait`, which does not wait: nothing
        // in any process works on the run, and nobody is left waiting for it.
        says_interrupted(engine.status(&run).unwrap().state());
        let waited = engine.wait(&run).unwrap();
        says_interrupted(waited.state());
        assert_eq!((waited.events().len(), engine.waiting_for(&run)), (0, 0));
        // `cancel` is not accepted: the run had ended.
        let Ok(Cancel::NotAccepted(state)) = engine.cancel(&run) else {
            panic!("the cancellation was accepted");
        };
        says_interrupted(&state);
    }
    // Another request under the id is a conflict, as with any run that ended.
    let other = Request::new(Call::new("other")).with_id(id("order-17"));
    let conflict = writing.request(&other).unwrap_err();
    assert!(
        matches!(conflict.refusal(), Some(Refusal::Conflict { .. })),
        "{conflict:?}"
    );
    assert_eq!(ran.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert_eq!(held(&world).2.len(), 1, "no run was made");
}

#[test]
fn the_questions_by_its_id_of_a_run_whose_admission_could_not_be_read_are_answered_as_interrupted()
{
    use whizbang_core::contract::Cancel;
    let world = World::new();
    plant(&world, "1000-1", &[(ADMITTED_NAME, b"no admission\n")], &[]);
    let (_opening, store, _) = reopened(&world);
    let engine = engine(store, Vec::new());
    let run = RunId::parse("1000-1").unwrap();
    says_interrupted(engine.status(&run).unwrap().state());
    says_interrupted(engine.wait(&run).unwrap().state());
    let Ok(Cancel::NotAccepted(state)) = engine.cancel(&run) else {
        panic!("the cancellation was accepted");
    };
    says_interrupted(&state);
}

// --- a crash inside the recovery of an interrupted run -----------------------------------------

/// Power is lost before each step of the recovery that ends a run and its request,
/// with every way the test kit's disk can lose what was not yet durable. What the
/// next open finds is the run as it was left, or ended with its record and its
/// ending both: never one without the other. That open ends it if it was not, and
/// the open after that publishes nothing: the run is ended once (rule 2).
fn crashes_inside_the_recovery_of_an_interrupted_run(part: usize, of: usize) {
    // What a recovery of one left run does to the disk, after the open.
    let (opened, steps) = {
        let world = World::new();
        leave_one(&world);
        let fs = Arc::new(ShimFs::new());
        let opening = Opening::open(&world.root(), fs.clone(), recovery()).unwrap();
        let opened = fs.events().len();
        opening.writer().unwrap();
        (opened, fs.events().len() - opened)
    };
    assert!(steps > 8, "{steps}");
    let (mut left, mut ended, mut crashes) = (0, 0, 0);
    for step in (0..steps).filter(|step| step % of == part) {
        let mut ways = vec![Gone::Unsynced, Gone::ContentOnly, Gone::Subset(0)];
        let mut next = 0;
        while next < ways.len() {
            let lost = ways[next];
            next += 1;
            crashes += 1;
            let at = format!("step {step}, {lost:?}");
            let world = World::new();
            let run = leave_one(&world);
            let fs = Arc::new(ShimFs::new().crash_before(opened + step, lost));
            let opening = Opening::open(&world.root(), fs.clone(), recovery()).unwrap();
            let _ = opening.writer();
            if lost == Gone::Subset(0) {
                let (dirs, names) = (fs.crash_choices(), fs.crash_names());
                assert!(dirs <= 10 && names <= 10, "{at}: {dirs} {names}");
                ways.extend((1..1u64 << dirs).map(Gone::Subset));
                ways.extend((0..1u64 << names).map(Gone::Names));
            }
            drop(opening);
            // Whole: the record and the id's ending are there together, or neither.
            let (_, names, runs) = held(&world);
            let has_record = runs[&run].contains_key("record");
            assert_eq!(
                names.contains_key("requests/order-17/ended"),
                has_record,
                "{at}"
            );
            if has_record {
                ended += 1;
            } else {
                left += 1;
            }
            // The next open ends it if it was not. What it is ended with is the one
            // record, whenever it was made; and another open after it adds nothing.
            drop(reopened(&world));
            let once = held(&world);
            let admission = Admission::from_bytes(&once.2[&run][ADMITTED_NAME]).unwrap();
            assert_eq!(
                once.2[&run]["record"],
                made_from(&admission).to_bytes(),
                "{at}"
            );
            assert!(once.1.contains_key("requests/order-17/ended"), "{at}");
            let (_opening, _, fs) = reopened(&world);
            assert_eq!(held(&world), once, "{at}: ended twice");
            let wrote = fs
                .events()
                .iter()
                .any(|event| matches!(event, Event::Write(_)));
            assert!(!wrote, "{at}: published twice");
        }
    }
    assert!(left > 0 && crashes > 10, "{left} {ended} {crashes}");
    let _ = ended;
}

#[test]
fn a_crash_at_any_event_of_the_recovery_of_an_interrupted_run_followed_by_another_open_ends_the_run_once_part_1_of_4()
 {
    crashes_inside_the_recovery_of_an_interrupted_run(0, 4);
}

#[test]
fn a_crash_at_any_event_of_the_recovery_of_an_interrupted_run_followed_by_another_open_ends_the_run_once_part_2_of_4()
 {
    crashes_inside_the_recovery_of_an_interrupted_run(1, 4);
}

#[test]
fn a_crash_at_any_event_of_the_recovery_of_an_interrupted_run_followed_by_another_open_ends_the_run_once_part_3_of_4()
 {
    crashes_inside_the_recovery_of_an_interrupted_run(2, 4);
}

#[test]
fn a_crash_at_any_event_of_the_recovery_of_an_interrupted_run_followed_by_another_open_ends_the_run_once_part_4_of_4()
 {
    crashes_inside_the_recovery_of_an_interrupted_run(3, 4);
}
