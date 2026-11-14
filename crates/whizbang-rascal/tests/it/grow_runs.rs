//! What a run of `grow` publishes (`SPEC.md` section 3, "What a run publishes"; ADR
//! 0008): the publication tests of the commit step and of cancellation, named for this
//! action and run on its own code.
//!
//! - A cancelled `grow` commits nothing.
//! - A failed `grow` commits nothing, and keeps its diagnostics.
//! - A completed outcome is not returned before the publication is durable.
//! - After a reopen, the mask and the completed status are both present.
//! - After a failure or an accepted cancellation, the store is closed and opened again:
//!   the run has its terminal status, no committed output and no completed status, and
//!   every diagnostic written before it ended is readable.
//!
//! The crash loop and its reading of the store are those of the core's commit tests
//! (`crates/whizbang-core/tests/it/contract/commit.rs`), and the barrier is the cancel
//! tests'.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use whizbang_core::artifact::{ArtifactId, Mask};
use whizbang_core::contract::record::Status;
use whizbang_core::contract::{
    Answer, CallError, Cancel, Ending, Executor, Failure, Outcome, Produced, Request, RequestId,
    RunId, RunState,
};
use whizbang_core::store::fs::Fs;
use whizbang_core::store::{Reader, Store};
use whizbang_rascal::grow::{DIAGNOSTIC, Grow};
use whizbang_testkit::barrier::Barriers;
use whizbang_testkit::fs_shim::{Lost, ShimFs};

use super::grow_action::{Around, Held, World, call, engine_over, files_under, fixture, held};

const ASKED: &str = "{\"px\":\"2.5\",\"down_px\":\"3\",\"radius_squared\":6,\"rows_down\":3}\n";
const READ: &str = "grow: read the mask";
const GREW: &str = "grow: the core went from 16 to 84 pixels; the blend is cleared";

/// The call every test here makes: the fixture grown by 2.5 and 3 rows down.
fn request(input: ArtifactId, id: &str) -> Request {
    Request::new(call(input, 2.5, Some(3.0))).with_id(RequestId::parse(id).unwrap())
}

/// The mask that call makes, worked out without a store.
fn expected() -> Mask {
    let grown = whizbang_rascal::grow::grow(fixture(), 2.5, Some(3.0), &|| false);
    grown.unwrap().unwrap().mask
}

/// The store, closed and opened again: every executor and writer of the test is gone,
/// a new writer has recovered whatever was left, and a reader reads what is there.
fn reopened(world: &World) -> Held {
    let fs: Arc<dyn Fs> = Arc::new(ShimFs::new());
    drop(Store::open(&world.root(), fs.clone()).unwrap());
    held(&world.root(), fs)
}

/// The ending recorded for a request id, when the store holds one.
fn ending(root: &Path, id: &str) -> Option<Status> {
    let reader = Reader::open(root, Arc::new(ShimFs::new())).unwrap();
    let generation = reader.snapshot().unwrap();
    let entry = generation.get(&RequestId::parse(id).unwrap().ending_name())?;
    Some(
        Ending::from_bytes(&reader.read(entry).unwrap())
            .unwrap()
            .status(),
    )
}

/// A run that ended and committed nothing: its status, no output among its names or in
/// its record, no mask in the store but the input, and its diagnostics readable.
fn ended_with_nothing(after: &Held, status: Status, journal: &[&str]) {
    let [run] = after.runs.as_slice() else {
        panic!("{} runs", after.runs.len());
    };
    let record = run.record.as_ref().expect("a record");
    assert_eq!(record.outcome().status, status);
    assert_ne!(record.outcome().status, Status::Completed);
    assert!(record.outputs().is_empty());
    assert!(run.outputs.is_empty(), "{:?}", run.outputs);
    assert!(
        run.names
            .iter()
            .all(|(name, _)| !name.starts_with("outputs/"))
    );
    let masks: Vec<_> = after.masks.iter().map(Mask::id).collect();
    assert_eq!(masks, [fixture().id()]);
    // Every diagnostic the record lists is among the run's names, read whole.
    for diagnostic in record.diagnostics() {
        let held = run.names.iter().find(|(name, _)| name == diagnostic.name());
        let (_, bytes) = held.unwrap_or_else(|| panic!("no {}", diagnostic.name()));
        assert_eq!(bytes.len() as u64, diagnostic.bytes());
    }
    assert_eq!(
        run.diagnostic(DIAGNOSTIC)
            .map(String::from_utf8_lossy)
            .as_deref(),
        Some(ASKED)
    );
    assert_eq!(run.journal(), journal);
}

// --- cancelled --------------------------------------------------------------------------------

#[test]
fn a_cancelled_grow_commits_nothing() {
    // A second caller cancels the run while `grow` is stopped at a barrier: before it
    // has started on the mask, and after it has made the mask it would hand back.
    for (made, journal) in [(false, &[READ][..]), (true, &[READ, GREW][..])] {
        let world = World::new();
        let input = world.import(&fixture());
        let store = world.store();
        let barriers = Barriers::new();
        let around = {
            let (early, late) = (barriers.clone(), barriers.clone());
            let mut around = Around::after(move |handed, _| {
                if made {
                    // The mask is made: every byte of the output exists.
                    assert!(handed.is_ok());
                    late.stop_at("busy");
                }
                handed
            });
            if !made {
                around = around.and_before(move |_| early.stop_at("busy"));
            }
            around
        };
        let first = engine_over(store.clone(), Arc::new(around));
        let second = engine_over(store.clone(), Arc::new(Grow::new()));
        whizbang_testkit::threads::scope(|scope| {
            let running = scope.spawn(|| first.request(&request(input, "r-1")));
            barriers.arrived("busy", 1);
            let run = held(&world.root(), world.fs.clone()).runs[0].id.clone();
            let run = RunId::parse(&run).unwrap();
            assert_eq!(
                second.status(&run).unwrap().state(),
                &RunState::NotEnded { here: true }
            );
            assert_eq!(second.cancel(&run), Ok(Cancel::Accepted));
            barriers.release("busy");
            let Ok(Answer::Ran(finished)) = running.join().unwrap() else {
                panic!("the request did not run");
            };
            assert_eq!(finished.outcome(), &Outcome::Cancelled);
        });
        drop((first, second, store));

        let after = reopened(&world);
        ended_with_nothing(&after, Status::Cancelled, journal);
        assert_eq!(ending(&world.root(), "r-1"), Some(Status::Cancelled));
    }
}

// --- failed -----------------------------------------------------------------------------------

#[test]
fn a_failed_grow_commits_nothing_and_keeps_its_diagnostics() {
    // Three failures, the last two once the output's bytes exist: `grow` cannot read
    // its mask; something fails after `grow` made its mask; and what is handed back is
    // not what the call returns, which the executor finds when it stages the outputs.
    type After = fn(Result<Produced, Failure>) -> Result<Produced, Failure>;
    let fails: After = |handed| {
        assert!(handed.is_ok());
        Err(Failure::error("after", "it broke once the mask was made"))
    };
    let twice: After = |handed| {
        let mask = fixture();
        Ok(handed.unwrap().with("mask", mask))
    };
    for (damaged, after, kind, journal) in [
        (true, None, "input", &[][..]),
        (false, Some(fails), "after", &[READ, GREW][..]),
        (false, Some(twice), "output", &[READ, GREW][..]),
    ] {
        let world = World::new();
        let input = world.import(&fixture());
        let root = world.root();
        let around = Around::after(move |handed, _| match after {
            Some(after) => after(handed),
            None => handed,
        })
        .and_before(move |_| {
            if damaged {
                let length = fixture().to_bytes().len() as u64;
                for path in files_under(&root.join("objects")) {
                    if path.metadata().unwrap().len() == length {
                        std::fs::write(path, b"not a mask").unwrap();
                    }
                }
            }
        });
        let engine = engine_over(world.store(), Arc::new(around));
        let Ok(Answer::Ran(finished)) = engine.request(&request(input, "r-1")) else {
            panic!("the request did not run");
        };
        let Outcome::Error { kind: found, .. } = finished.outcome() else {
            panic!("{:?}", finished.outcome());
        };
        assert_eq!(found, kind);
        assert!(finished.outcome().outputs().is_empty());
        drop(engine);

        // The input's file is put back before the store is read whole again.
        if damaged {
            for path in files_under(&world.root().join("objects")) {
                if std::fs::read(&path).unwrap() == b"not a mask" {
                    std::fs::write(path, fixture().to_bytes()).unwrap();
                }
            }
        }
        let after = reopened(&world);
        ended_with_nothing(&after, Status::Error, journal);
        assert_eq!(ending(&world.root(), "r-1"), Some(Status::Error));
    }
}

// --- completed --------------------------------------------------------------------------------

#[test]
fn after_a_reopen_the_mask_and_the_completed_status_are_both_present() {
    let world = World::new();
    let input = world.import(&fixture());
    let engine = world.engine();
    let Ok(Answer::Ran(finished)) = engine.request(&request(input, "r-1")) else {
        panic!("the request did not run");
    };
    let made = finished.outcome().outputs()[0].id();
    assert_eq!(made, expected().id());
    drop(engine);

    let after = reopened(&world);
    let [run] = after.runs.as_slice() else {
        panic!("{} runs", after.runs.len());
    };
    let record = run.record.as_ref().expect("a record");
    assert_eq!(record.outcome().status, Status::Completed);
    assert_eq!(record.outputs()[0].artifact, made.0);
    assert_eq!(run.outputs, ["outputs/mask/0"]);
    let grown = after.masks.iter().find(|mask| mask.id() == made).unwrap();
    assert_eq!(grown.core(), expected().core());
    assert_eq!(ending(&world.root(), "r-1"), Some(Status::Completed));
    assert_eq!(run.journal(), [READ, GREW]);

    // The reopened store answers the same request with that run, and takes the mask
    // as the input of the next call.
    let engine = world.engine();
    let Ok(Answer::Repeat(earlier)) = engine.request(&request(input, "r-1")) else {
        panic!("not answered as a repeat");
    };
    assert_eq!(earlier.run(), finished.run());
    assert_eq!(
        earlier.state(),
        &RunState::Ended(finished.outcome().clone())
    );
    let next = engine.submit(&call(made, 1.0, None)).unwrap();
    assert_eq!(next.outcome().name(), "completed");
}

/// How many calls that change the disk a run of `grow` makes before `grow` comes back,
/// and how many after: the second number is its commit.
fn the_calls_of_a_commit() -> (usize, usize) {
    let world = World::new();
    let input = world.import(&fixture());
    let came_back = Arc::new(AtomicUsize::new(0));
    let noting = {
        let (fs, came_back) = (world.fs.clone(), came_back.clone());
        Around::after(move |handed, _| {
            came_back.store(fs.events().len(), Ordering::SeqCst);
            handed
        })
    };
    let engine = engine_over(world.store(), Arc::new(noting));
    engine.submit(&call(input, 2.5, Some(3.0))).unwrap();
    let start = came_back.load(Ordering::SeqCst);
    (start, world.fs.events().len() - start)
}

/// A world in which the fixture is imported, on this file system.
fn world_on(fs: ShimFs) -> (World, ArtifactId) {
    let world = World::on(fs);
    let input = world.import(&fixture());
    (world, input)
}

#[test]
fn a_grow_crashed_inside_its_commit_is_not_reported_before_its_publication_is_durable() {
    // `grow` has handed back its mask. Power is lost before each call that changes the
    // disk from then on, with each way a disk can lose what was not yet durable: every
    // name undone, every name kept with its content cut, every choice of directories,
    // and every choice of the names outside `staging/`. Whatever survives is the run as
    // it was admitted, with no mask and no status, or the mask with the completed
    // status; and the run was reported only where both survive.
    let (start, steps) = the_calls_of_a_commit();
    assert!(steps > 8, "{steps}");
    let made = expected().id();
    let mut outcomes = BTreeSet::new();
    let (mut crashes, mut reported) = (0, 0);
    for step in 0..steps {
        let mut ways = vec![Lost::Unsynced, Lost::ContentOnly, Lost::Subset(0)];
        let mut next = 0;
        while next < ways.len() {
            let lost = ways[next];
            next += 1;
            crashes += 1;
            let (world, input) = world_on(ShimFs::new().crash_before(start + step, lost));
            let engine = world.engine();
            let answer = engine.submit(&call(input, 2.5, Some(3.0)));
            assert!(world.fs.crashed(), "step {step}");
            if lost == Lost::Subset(0) {
                let (dirs, named) = (world.fs.crash_choices(), world.fs.crash_named());
                let staging = world.root().join("staging");
                let outside: Vec<usize> = (0..named.len())
                    .filter(|at| !named[*at].starts_with(&staging))
                    .collect();
                assert!(dirs <= 6 && outside.len() <= 6, "step {step}: {named:?}");
                ways.extend((1..1u64 << dirs).map(Lost::Subset));
                for choice in 1..1u64 << outside.len() {
                    let names = (0..outside.len()).filter(|at| choice >> at & 1 == 1);
                    ways.push(Lost::Names(names.map(|at| 1u64 << outside[at]).sum()));
                }
            }
            drop(engine);

            let after = reopened(&world);
            let at = format!("step {step}, {lost:?}");
            let [run] = after.runs.as_slice() else {
                panic!("{at}: {} runs", after.runs.len());
            };
            let has = after.masks.iter().any(|mask| mask.id() == made);
            match &run.record {
                None => {
                    assert!(!has && run.outputs.is_empty(), "{at}");
                    assert_eq!(run.names.len(), 1, "{at}");
                    assert!(
                        matches!(answer, Err(CallError::Store(_))),
                        "{at}: a run was reported whose commit did not survive: {answer:?}"
                    );
                    outcomes.insert("neither");
                }
                Some(record) => {
                    assert!(has, "{at}");
                    assert_eq!(record.outcome().status, Status::Completed, "{at}");
                    assert_eq!(record.outputs()[0].artifact, made.0, "{at}");
                    assert_eq!(run.outputs, ["outputs/mask/0"], "{at}");
                    assert_eq!(
                        run.diagnostic(DIAGNOSTIC)
                            .map(String::from_utf8_lossy)
                            .as_deref(),
                        Some(ASKED),
                        "{at}"
                    );
                    outcomes.insert("both");
                    if let Ok(finished) = &answer {
                        assert_eq!(finished.outcome().outputs()[0].id(), made, "{at}");
                        reported += 1;
                    }
                }
            }
        }
    }
    assert_eq!(
        outcomes.into_iter().collect::<Vec<_>>(),
        ["both", "neither"]
    );
    assert!(crashes > 5 * steps, "{crashes} crashes over {steps} steps");
    assert!(reported > 0);

    // And the other way round: the moment a completed run is reported, power is lost
    // and everything that was not durable goes. The mask and the status are there.
    let (world, input) = world_on(ShimFs::new());
    let engine = world.engine();
    let finished = engine.submit(&call(input, 2.5, Some(3.0))).unwrap();
    assert_eq!(finished.outcome().outputs()[0].id(), made);
    world.fs.crash(Lost::Unsynced);
    drop(engine);
    let after = reopened(&world);
    assert!(after.masks.iter().any(|mask| mask.id() == made));
    let record = after.runs[0].record.as_ref().expect("a record");
    assert_eq!(record.outcome().status, Status::Completed);
}

#[test]
fn a_commit_of_grow_that_the_store_refuses_publishes_no_mask_and_no_status_or_both() {
    // The store fails at each call of the commit in turn, once the mask's bytes are
    // being staged, without a crash. The answer is the store's failure, with no path in
    // it. What a reader then sees is the run as it was admitted and no mask, or, where
    // the commit became visible and its last sync failed, all of it. A run whose commit
    // the store refused has no terminal status: it is admitted and not ended, as ADR
    // 0008 says of a run whose end could not be written, and nothing of it is committed.
    let (start, steps) = the_calls_of_a_commit();
    let made = expected().id();
    let mut seen = BTreeSet::new();
    for step in 0..steps {
        let (world, input) = world_on(ShimFs::new());
        let engine = world.engine();
        world.fs.refuse_before(start + step);
        let answer = engine.submit(&call(input, 2.5, Some(3.0)));
        drop(engine);
        let after = reopened(&world);
        let run = &after.runs[0];
        let has = after.masks.iter().any(|mask| mask.id() == made);
        match answer {
            Err(CallError::Store(failure)) => {
                let said = CallError::Store(failure).to_string();
                assert!(!said.contains(world.root().to_str().unwrap()), "{said}");
                let unconfirmed = said.contains("not confirmed durable");
                assert_eq!(run.record.is_some(), unconfirmed, "step {step}: {said}");
                assert_eq!(has, unconfirmed, "step {step}");
                assert_eq!(run.outputs.len(), usize::from(unconfirmed), "step {step}");
                if let Some(record) = &run.record {
                    assert_eq!(record.outcome().status, Status::Completed);
                }
                seen.insert(unconfirmed);
            }
            // Clearing the staging away, after the commit is durable, can fail without
            // failing the run.
            Ok(finished) => {
                assert_eq!(step, steps - 1);
                assert_eq!(finished.outcome().outputs()[0].id(), made);
                assert!(has && run.record.is_some());
            }
            other => panic!("step {step}: {other:?}"),
        }
    }
    assert_eq!(seen.into_iter().collect::<Vec<_>>(), [false, true]);
}

// --- the record, against the spec's list ------------------------------------------------------

/// The record of the one run of `grow` a world holds, as the JSON that is stored, with
/// what the store holds of the run beside it.
fn record_of(world: &World) -> (serde_json::Value, Held) {
    let after = reopened(world);
    let [run] = after.runs.as_slice() else {
        panic!("{} runs", after.runs.len());
    };
    let stored = run.names.iter().find(|(name, _)| name == "record");
    let document = serde_json::from_slice(&stored.expect("a record").1).unwrap();
    (document, after)
}

#[test]
fn the_records_of_grow_completed_failed_and_cancelled_hold_what_the_spec_lists() {
    // `SPEC.md` section 3, "What every run records", group by group, for a run of
    // `grow` that completed, one that failed and one that was cancelled, each through
    // an executor with a ledger. A group that `grow` has nothing for is there and
    // empty: it asks no capability, so it has no model, no paid request and no
    // egress; it calls no action, so it has no step; and it asks no judge.
    use serde_json::{Value as Json, json};
    use whizbang_testkit::ledger::FakeLedger;
    let failing = || Around::after(|_, _| Err(Failure::error("made", "the test failed it")));
    let cancelling = || Around::before(|context| context.cancellation().cancel());
    let cases: [(&str, Arc<dyn whizbang_core::contract::Action>); 3] = [
        ("completed", Arc::new(Grow::new())),
        ("error", Arc::new(failing())),
        ("cancelled", Arc::new(cancelling())),
    ];
    for (status, action) in cases {
        let world = World::new();
        let input = world.import(&fixture());
        let ledger = Arc::new(FakeLedger::new());
        let engine = engine_over(world.store(), action).with_ledger(ledger.clone());
        let Ok(Answer::Ran(finished)) = engine.request(&request(input, "exit-1")) else {
            panic!("{status}: the request did not run");
        };
        assert_eq!(finished.outcome().name(), status);
        drop(engine);
        let (record, after) = record_of(&world);
        let run = &after.runs[0];

        // Identity: the run, the request, no parent, the version.
        assert_eq!(
            record["identity"],
            json!({"run": finished.run().as_str(), "request": "exit-1", "parent": null,
                   "whizbang": env!("CARGO_PKG_VERSION")}),
            "{status}"
        );
        // What was asked: the action, its exact arguments, no settings yet, and the
        // digest of the config, which is of no bytes.
        assert_eq!(
            record["asked"],
            json!({"action": "grow", "version": "1.0.0", "settings": [],
                   "arguments": [{"name": "px", "value": {"number": 2.5}},
                                 {"name": "down_px", "value": {"number": 3.0}}],
                   "config": whizbang_core::identity::Digest::of_bytes(b"").to_string()}),
            "{status}"
        );
        // Inputs and outputs: the input with its binding; a reference to the output
        // of the run that completed, and none of the others.
        assert_eq!(
            record["inputs"],
            json!([{"port": "mask", "type": "mask", "artifact": input.to_string(),
                    "binding": fixture().binding().id().to_string()}]),
            "{status}"
        );
        let outputs = match status {
            "completed" => json!([{"port": "mask", "type": "mask",
                                   "artifact": expected().id().to_string()}]),
            _ => json!([]),
        };
        assert_eq!(record["outputs"], outputs, "{status}");
        // Outcome: the status; for the error its type and message; no refusal.
        let error = match status {
            "error" => json!({"kind": "made", "message": "the test failed it"}),
            _ => Json::Null,
        };
        assert_eq!(
            record["outcome"],
            json!({"status": status, "error": error, "invalid": null, "refusals": [],
                   "refusals_omitted": 0}),
            "{status}"
        );
        // Time: start and end; no step, call or request to give a duration.
        let (start, end) = (&record["time"]["start_ms"], &record["time"]["end_ms"]);
        assert!(start.as_u64().unwrap() > 0 && start.as_u64() <= end.as_u64());
        assert_eq!(record["time"]["durations"], json!([]), "{status}");
        // Spend: the ledger took part, so an amount was reserved for the run: none,
        // since `grow` asks nothing that costs. It is in the store under the run.
        assert_eq!(
            record["spend"],
            json!({"reserved_micros": 0, "requests": []}),
            "{status}"
        );
        let reader = Reader::open(&world.root(), Arc::new(ShimFs::new())).unwrap();
        let reserved = ledger.reservations(&reader).unwrap();
        let [reservation] = reserved.as_slice() else {
            panic!("{status}: {} reservations", reserved.len());
        };
        assert_eq!(
            (reservation.run(), reservation.micros()),
            (finished.run(), 0)
        );
        // Models, egress, steps and judge: there, and empty.
        for group in ["models", "egress", "steps", "judge"] {
            assert_eq!(record[group], json!([]), "{status}: {group}");
        }
        // Diagnostics: the journal and what the action wrote, each listed with the
        // digest and length of what the store holds under its name, whatever the
        // outcome.
        let listed = record["diagnostics"].as_array().unwrap();
        let names: Vec<&str> = listed.iter().map(|d| d["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["diagnostics/grow.json", "journal"], "{status}");
        for diagnostic in listed {
            let name = diagnostic["name"].as_str().unwrap();
            let (_, bytes) = run.names.iter().find(|(held, _)| held == name).unwrap();
            assert_eq!(diagnostic["bytes"].as_u64(), Some(bytes.len() as u64));
            let digest = whizbang_core::store::Digest::of_bytes(bytes);
            assert_eq!(diagnostic["digest"], format!("sha256:{}", digest.hex()));
        }
        // And nothing else: the eleven groups of the list, of which inputs and
        // outputs are two fields, and the record's kind.
        assert_eq!(record["record"], "run/2");
        assert_eq!(record.as_object().unwrap().len(), 13, "{status}");
    }
}
