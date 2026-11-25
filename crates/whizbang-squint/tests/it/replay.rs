//! `replay::…`: `check` replayed (plan m7, task R1; ADR 0049), offline from what its run
//! recorded and live under the executor's judge: a tool file it called, one whose call
//! failed, and a turn cut off at its backend's output ceiling. Every judge is a fake.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::json;
use whizbang_core::contract::record::RunRecord;
use whizbang_core::contract::replay::{Replay, Source};
use whizbang_core::contract::{
    Answer, CallError, Engine, Executor, Outcome, Refusal, RunId, RunState, Secrets,
};
use whizbang_core::store::Reader;
use whizbang_testkit::capabilities::FakeJudge;

use super::check::{PROBE_DIGEST, of_pair, paired, probing, submitting, turns};
use super::world::*;

/// The executor's actions, each at its version: what a replay's root must be.
fn shipped_of(engine: &Engine) -> BTreeMap<String, String> {
    (engine.registry().descriptors())
        .map(|d| (d.name().to_owned(), d.version().to_owned()))
        .collect()
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

/// What a replay's root noted of where it stopped, when it stopped.
fn note(world: &World, run: &RunId) -> Option<String> {
    let reader = Reader::open(&world.root(), world.fs.clone()).unwrap();
    let generation = reader.snapshot().unwrap();
    let name = whizbang_core::contract::replay::note_name();
    let entry = generation.run(run.as_str())?.get(&name)?;
    Some(String::from_utf8(reader.read(entry).unwrap()).unwrap())
}

fn refused(engine: &Engine, replay: &Replay) -> String {
    match engine.start_replay(replay) {
        Err(CallError::Refused(Refusal::NotReplayed { why, .. })) => why,
        other => panic!("{other:?}"),
    }
}

/// The judge's call of the tool file `probe`.
fn probed() -> serde_json::Value {
    json!({"tool_call": {"id": "p1", "name": "probe", "arguments":
        {"source": "source", "candidate": "candidate", "phrase": "door"}}})
}

#[test]
fn a_check_that_called_a_tool_file_is_refused_offline_and_replayed_live_only_with_the_same_digest()
{
    let (mut world, source, candidate, _) = paired();
    let probe = probing(&mut world);
    // The judge is chosen by name, so a live replay makes the same choice.
    let call = of_pair(
        &source,
        &candidate,
        &[("tools", "view,probe"), ("judge", "fake")],
    );
    let engine = world.engine(judge(&submitting(&[probed()]), &[]), Secrets::none());
    let original = engine.submit(&call).unwrap();
    assert!(
        matches!(original.outcome(), Outcome::Completed { .. }),
        "{:?}",
        original.outcome()
    );
    let shipped = shipped_of(&engine);
    drop(engine);
    let of = Replay::of(Source::Run(original.run().clone()));
    // Offline: refused before it starts, naming the file; it never runs.
    let engine = world
        .engine(judge(&submitting(&[]), &[]), Secrets::none())
        .with_shipped(shipped.clone());
    let why = refused(&engine, &of);
    assert!(why.contains("called the tool file probe"), "{why}");
    assert!(why.contains("an offline replay never runs one"), "{why}");
    // Live, under a config whose file of that name is another digest: refused.
    let other = BTreeMap::from([("probe".to_owned(), format!("sha256:{}", "cd".repeat(32)))]);
    let why = refused(&engine.clone().with_tool_files(other), &of.clone().live());
    assert!(why.contains("the tool file probe at"), "{why}");
    drop(engine);
    assert_eq!(probe.ran.lock().unwrap().len(), 1);
    // Live, with the file at the digest the record noted: replayed, and the file runs.
    let same = BTreeMap::from([("probe".to_owned(), PROBE_DIGEST.to_owned())]);
    let engine = world
        .engine(judge(&submitting(&[probed()]), &[]), Secrets::none())
        .with_shipped(shipped)
        .with_tool_files(same);
    let (_, outcome) = replayed(&engine, &of.live());
    assert!(matches!(outcome, Outcome::Completed { .. }), "{outcome:?}");
    assert_eq!(probe.ran.lock().unwrap().len(), 2);
}

#[test]
fn a_tool_file_call_that_failed_in_the_original_runs_no_file_offline() {
    // The file failed, so its record notes no digest and the replay is not refused at
    // its start: the replay's context refuses the call where it is dispatched, no file
    // runs, and the judge is then sent another request than the original was: diverged.
    let (mut world, source, candidate, _) = paired();
    let probe = Arc::new(super::check::Probe {
        fails: true,
        ..Default::default()
    });
    world.toolbox = whizbang_squint::check::Toolbox::with_files(probe.clone());
    let call = of_pair(&source, &candidate, &[("tools", "view,probe")]);
    let engine = world.engine(judge(&submitting(&[probed()]), &[]), Secrets::none());
    let original = engine.submit(&call).unwrap();
    assert!(
        matches!(original.outcome(), Outcome::Completed { .. }),
        "{:?}",
        original.outcome()
    );
    let noted = |record: &RunRecord| {
        (record.asked().settings.iter()).any(|s| s.name().starts_with("tool_file:"))
    };
    let left = left(&world.root(), world.fs.clone(), original.run());
    assert!(!noted(left.record()));
    let shipped = shipped_of(&engine);
    let engine = engine.with_shipped(shipped);
    assert_eq!(probe.ran.lock().unwrap().len(), 1);
    let (run, _) = replayed(&engine, &Replay::of(Source::Run(original.run().clone())));
    assert_eq!(
        probe.ran.lock().unwrap().len(),
        1,
        "no file ran in the replay"
    );
    let said = note(&world, &run).expect("the replay stopped");
    assert!(said.starts_with("diverged judge request 2:"), "{said}");
}

/// A judge that answers each turn of this script under an output ceiling, when given.
fn under_ceiling(script: &str, ceiling: Option<u32>) -> Arc<FakeJudge> {
    let read = whizbang_core::capability::written::read_script(script.as_bytes()).unwrap();
    let turns = (read.into_iter())
        .map(|turn| turn.map(|turn| ceiling.map_or(turn.clone(), |c| turn.under_output_ceiling(c))))
        .collect();
    Arc::new(FakeJudge::scripted(turns))
}

#[test]
fn an_answer_cut_at_its_ceiling_replays_as_it_ran() {
    let cut = (json!([{"text": "I will"}]), "max_tokens");
    // Cut at its backend's output ceiling, which a retry would not raise: the check
    // ends as it did, the ceiling read back from the stored exchange.
    let (world, source, candidate, _) = paired();
    let call = of_pair(&source, &candidate, &[]);
    let engine = world.engine(
        under_ceiling(&turns(&[cut.clone()]), Some(1024)),
        Secrets::none(),
    );
    let ceiling = engine.submit(&call).unwrap();
    let Outcome::Error { kind, .. } = ceiling.outcome() else {
        panic!("{:?}", ceiling.outcome());
    };
    assert_eq!(kind, "judge_output_ceiling");
    let shipped = shipped_of(&engine);
    let engine = engine.with_shipped(shipped);
    let (run, outcome) = replayed(&engine, &Replay::of(Source::Run(ceiling.run().clone())));
    assert_eq!(&outcome, ceiling.outcome());
    assert_eq!(note(&world, &run), None);
    drop(engine);
    // Cut with no ceiling stated: asked again with a larger limit, as it was, and
    // answered from the next stored turn.
    let (world, source, candidate, _) = paired();
    let call = of_pair(&source, &candidate, &[]);
    let script = submitting(&[]);
    let mut entries: serde_json::Value = serde_json::from_str(&script).unwrap();
    let first =
        serde_json::from_str::<serde_json::Value>(&turns(&[cut])).unwrap()["entries"][0].clone();
    entries["entries"].as_array_mut().unwrap().insert(0, first);
    let engine = world.engine(under_ceiling(&entries.to_string(), None), Secrets::none());
    let retried = engine.submit(&call).unwrap();
    assert!(
        matches!(retried.outcome(), Outcome::Completed { .. }),
        "{:?}",
        retried.outcome()
    );
    let shipped = shipped_of(&engine);
    let engine = engine.with_shipped(shipped);
    let (run, outcome) = replayed(&engine, &Replay::of(Source::Run(retried.run().clone())));
    assert_eq!(&outcome, retried.outcome());
    assert_eq!(note(&world, &run), None);
}
