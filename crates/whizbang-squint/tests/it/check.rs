//! `check` against a scripted judge (task A11 of checkpoint 1A; ADR 0009): the ten
//! transcripts of the plan, each a script read from data, then what the plan asks
//! beside them: the tool list after admission, a cancelled and a completed run across
//! a reopen, and what a judge that is not trusted may send.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use serde_json::{Value as Json, json};
use whizbang_core::artifact::{ArtifactId, Image};
use whizbang_core::capability::{
    Bounds, Declaration, Excluded, Fault, Judge, JudgeRequest, Part, ToolResult, Turn,
};
use whizbang_core::contract::record::{Decision, Judged, RunRecord, Status, Stop};
use whizbang_core::contract::{
    Answer, Call, CallError, Cancel, Executor, Finished, Outcome, RefusalKind, Request, RunId,
    RunState, Secrets, Value,
};
use whizbang_core::identity::Digest;
use whizbang_core::store::Kind as Stored;
use whizbang_core::store::fs::Fs;
use whizbang_squint::check::{CALLS_LIMIT, RETRY_FACTOR, TURN_TEXT_LIMIT, TURNS_LIMIT};
use whizbang_testkit::barrier::Barriers;
use whizbang_testkit::capabilities::FakeJudge;
use whizbang_testkit::fs_shim::{Lost, ShimFs};

use super::support::*;
use super::world::*;

/// The crop `view` cuts of this image for this box, worked out apart from any run.
fn crop_of(image: &Image, bbox: [f64; 4]) -> Image {
    look(image, bbox).image
}

const HALF: [f64; 4] = [0.25, 0.25, 0.75, 0.75];
const CORNER: [f64; 4] = [0.0, 0.0, 0.5, 0.5];

/// Runs `check` on the photograph against this script, and gives the run as the
/// executor reported it, what the store holds of it once the executor is gone and the
/// store is opened anew, and the judge.
fn run(
    world: &World,
    script: &str,
    names: &[(&str, ArtifactId)],
) -> (Finished, Left, Arc<FakeJudge>) {
    let judge = judge(script, names);
    let executor = world.engine(judge.clone(), Secrets::none());
    let finished = executor.submit(&world.check()).unwrap();
    drop(executor);
    let left = left(&world.root(), world.fs.clone(), finished.run());
    (finished, left, judge)
}

/// The tool results the judge was sent in its request of this number, from 0: the
/// parts of the last message.
fn results(judge: &FakeJudge, request: usize) -> Vec<ToolResult> {
    let received = judge.received();
    let parts = received[request]
        .messages()
        .last()
        .unwrap()
        .parts()
        .to_vec();
    let result = |part: Part| match part {
        Part::ToolResult(result) => result,
        other => panic!("{other:?}"),
    };
    parts.into_iter().map(result).collect()
}

fn steps(record: &RunRecord) -> Vec<(&str, Option<Status>)> {
    record
        .steps()
        .iter()
        .map(|step| (step.action(), step.status()))
        .collect()
}

fn refusals(record: &RunRecord) -> Vec<(Option<&str>, RefusalKind)> {
    let refusals = record.outcome().refusals.iter();
    refusals
        .map(|refused| (refused.action(), refused.kind()))
        .collect()
}

/// No reference of the run is a committed output: it holds diagnostics alone.
fn commits_nothing(left: &Left) {
    assert!(
        left.names
            .values()
            .all(|(kind, _)| *kind == Stored::Diagnostic)
    );
    assert!(!left.names.keys().any(|name| name.starts_with("outputs/")));
    if let Some(record) = &left.record {
        assert!(record.outputs().is_empty());
    }
}

// --- the ten transcripts ----------------------------------------------------------------------

const LOOK_CLOSER: &str = r#"{"script":"judge-script/1","entries":[
  {"turn":{"parts":[{"text":"I will look at the middle."},
      {"tool_call":{"id":"c1","name":"view","arguments":{"image":"$photo","bbox":[0.25,0.25,0.75,0.75]}}}],
    "stop":"tool_use","usage":{"input_tokens":900,"output_tokens":40,"cache_read_tokens":0,"cache_write_tokens":0},"request_id":"r-1"}},
  {"turn":{"parts":[{"tool_call":{"id":"c2","name":"view","arguments":{"image":"$crop","bbox":[0,0,0.5,0.5]}}}],
    "stop":"tool_use","usage":null,"request_id":null}},
  {"turn":{"parts":[{"text":"pass\nThe corner is clean."}],"stop":"end_turn","usage":null,"request_id":null}}]}"#;

#[test]
fn look_then_look_closer_at_part_of_the_first_crop_then_decide() {
    let world = World::new();
    let first = crop_of(&world.photo, HALF);
    let second = crop_of(&first, CORNER);
    let names = [("photo", world.photo.id()), ("crop", first.id())];
    let (finished, left, judge) = run(&world, LOOK_CLOSER, &names);

    // Completed, with a verdict; and it commits no output: the verdict is in its record.
    assert_eq!(
        finished.outcome(),
        &Outcome::Completed {
            outputs: Vec::new()
        }
    );
    let record = left.record();
    assert_eq!(record.outcome().status, Status::Completed);
    assert_eq!(
        record.verdict(),
        Some((Decision::Pass, "The corner is clean."))
    );
    commits_nothing(&left);
    // Two `view` calls are recorded, each a run of its own that completed.
    let completed = Some(Status::Completed);
    assert_eq!(steps(record), [("view", completed), ("view", completed)]);
    assert!(record.outcome().refusals.is_empty());
    // Both crops are kept as evidence: diagnostics of this run, each the image it is.
    assert_eq!(left.evidence(1).id(), first.id());
    assert_eq!(left.evidence(2).id(), second.id());
    assert_eq!(rgba_of(&left.evidence(2)), rgba_of(&second));
    assert!(!left.has("evidence-003"));
    // The crop the first call returned was readable in the second turn: the judge
    // was shown it, named it, and was shown the crop of it.
    assert_eq!(judge.requests(), 3);
    let shown = results(&judge, 1);
    assert_eq!(shown[0].call(), "c1");
    assert!(!shown[0].is_error());
    assert_eq!(shown[0].images()[0].digest(), first.identity().pixels);
    assert!(
        shown[0].text().contains(&first.id().to_string()),
        "{}",
        shown[0].text()
    );
    let closer = results(&judge, 2);
    assert_eq!(closer[0].images()[0].digest(), second.identity().pixels);
    // The record's judge group: three turns and the verdict, each turn with its
    // exchange; and three requests to the model the judge declares.
    assert_eq!(record.judge().len(), 4);
    assert_eq!(record.models().len(), 3);
    assert!(
        record
            .models()
            .iter()
            .all(|model| model.model == "fake-judge")
    );
    let first_exchange = left.exchange(1);
    assert_eq!(
        first_exchange["answer"]["turn"]["parts"][0]["text"],
        "I will look at the middle."
    );
    let image = &first_exchange["messages"][0]["parts"][1]["image"];
    assert_eq!(
        image["pixels"],
        json!(world.photo.identity().pixels.to_string())
    );
}

/// A body no JSON library would write again as it is.
const BODY: &str = "{\"sig\":\"q7mark9x\", \"n\": 1.50,\"t\":\"caf\u{e9}\"}";

const TWO_CALLS: &str = r#"{"script":"judge-script/1","entries":[
  {"turn":{"parts":[
      {"tool_call":{"id":"c1","name":"view","arguments":{"image":"$photo","bbox":[0.25,0.25,0.75,0.75]}}},
      {"opaque":{"adapter":"fake","body":"{\"sig\":\"q7mark9x\", \"n\": 1.50,\"t\":\"café\"}"}},
      {"tool_call":{"id":"c2","name":"view","arguments":{"image":"$photo","bbox":[0,0,0.5,0.5]}}}],
    "stop":"tool_use","usage":null,"request_id":null}},
  {"turn":{"parts":[{"text":"fail\nThe corner is torn."}],"stop":"end_turn","usage":null,"request_id":null}}]}"#;

#[test]
fn two_tool_calls_in_one_turn_with_opaque_provider_content_between_them() {
    let world = World::new();
    let (_, left, judge) = run(&world, TWO_CALLS, &[("photo", world.photo.id())]);
    let record = left.record();
    assert_eq!(
        record.verdict(),
        Some((Decision::Fail, "The corner is torn."))
    );
    // Both were dispatched, in the order the judge made them.
    assert_eq!(steps(record).len(), 2);
    assert_eq!(left.evidence(1).id(), crop_of(&world.photo, HALF).id());
    assert_eq!(left.evidence(2).id(), crop_of(&world.photo, CORNER).id());
    let answered: Vec<_> = results(&judge, 1)
        .iter()
        .map(|r| r.call().to_owned())
        .collect();
    assert_eq!(answered, ["c1", "c2"]);
    // The opaque content went back to the judge unchanged, byte for byte, in its
    // place between the two calls.
    let received = judge.received();
    let said_back = received[1].messages()[1].parts();
    let kinds: Vec<_> = said_back.iter().map(|part| format!("{part:?}")).collect();
    assert!(
        kinds[0].starts_with("ToolCall(c1") && kinds[2].starts_with("ToolCall(c2"),
        "{kinds:?}"
    );
    let Part::Opaque(part) = &said_back[1] else {
        panic!("{kinds:?}");
    };
    assert_eq!(part.body().as_bytes(), BODY.as_bytes());
    // And its body is in nothing the run stored: its digest and its length are.
    let kept = &left.exchange(2)["messages"][1]["parts"][1]["opaque_withheld"];
    assert_eq!(
        kept["digest"],
        json!(Digest::of_bytes(BODY.as_bytes()).to_string())
    );
    for (path, bytes) in files_under(&world.root()) {
        assert!(
            !bytes.windows(8).any(|mark| mark == b"q7mark9x"),
            "{path:?}"
        );
    }
}

const REJECTED: &str = r#"{"script":"judge-script/1","entries":[
  {"turn":{"parts":[
      {"tool_call":{"id":"c1","name":"view","arguments":{"image":"$photo","bbox":[0.5,0.5,0.501,0.501]}}},
      {"tool_call":{"id":"c2","name":"view","arguments":{"image":"$photo","bbox":"all of it"}}},
      {"tool_call":{"id":"c3","name":"view","arguments":{"image":"the photo","bbox":[0,0,1,1]}}}],
    "stop":"tool_use","usage":null,"request_id":null}},
  {"turn":{"parts":[{"text":"unsure"}],"stop":"end_turn","usage":null,"request_id":null}}]}"#;

#[test]
fn a_tool_call_that_the_tool_rejects() {
    // The first call reaches the tool, which rejects the box as too small. The other
    // two do not make a call at all. Each error goes back as its call's result, and
    // the run goes on to its verdict.
    let world = World::new();
    let (finished, left, judge) = run(&world, REJECTED, &[("photo", world.photo.id())]);
    assert_eq!(finished.outcome().name(), "completed");
    let said = results(&judge, 1);
    assert!(
        said.iter()
            .all(|result| result.is_error() && result.images().is_empty())
    );
    assert!(
        said[0].text().starts_with("rejected: bbox"),
        "{}",
        said[0].text()
    );
    assert!(
        said[0].text().contains("at least 8x8 are needed"),
        "{}",
        said[0].text()
    );
    assert_eq!(
        said[1].text(),
        "not called: bbox is four numbers: left, top, right and bottom"
    );
    assert_eq!(
        said[2].text(),
        "not called: image is not the name of an image you were shown"
    );
    let record = left.record();
    assert_eq!(steps(record), [("view", Some(Status::Invalid))]);
    assert_eq!(record.verdict(), Some((Decision::Unsure, "")));
    assert!(!left.has("evidence-001"));
}

const CUT_OFF: &str = r#"{"script":"judge-script/1","entries":[
  {"turn":{"parts":[{"text":"pass\nThe label rea"},
      {"tool_call":{"id":"c1","name":"view","arguments":{"image":"$photo","bbox":[0,0,1,1]}}}],
    "stop":"max_tokens","usage":null,"request_id":null}},
  {"turn":{"parts":[{"text":"fail\nThe label reads wrong."}],"stop":"end_turn","usage":null,"request_id":null}}]}"#;

#[test]
fn a_response_cut_off_by_the_token_limit_then_a_retry_with_a_larger_limit() {
    let world = World::new();
    let (_, left, judge) = run(&world, CUT_OFF, &[("photo", world.photo.id())]);
    let record = left.record();
    // The run completes, with the verdict of the turn that was whole.
    assert_eq!(
        record.verdict(),
        Some((Decision::Fail, "The label reads wrong."))
    );
    // The retry asked for more tokens, and was sent the same conversation: the
    // cut-off turn is not in it, and the call it held was never dispatched.
    let received = judge.received();
    assert_eq!(received[0].controls().max_output_tokens(), 1024);
    assert_eq!(
        received[1].controls().max_output_tokens(),
        1024 * RETRY_FACTOR
    );
    assert_eq!(received[1].messages(), received[0].messages());
    assert!(record.steps().is_empty() && !left.has("evidence-001"));
    // It is recorded: marked as left out in the record, and whole in its exchange.
    let turn = |at: usize| match &record.judge()[at] {
        Judged::Turn {
            stop,
            excluded,
            max_output_tokens,
            ..
        } => (*stop, *excluded, *max_output_tokens),
        other => panic!("{other:?}"),
    };
    assert_eq!(
        turn(0),
        (Some(Stop::MaxTokens), Some(Excluded::Truncated), 1024)
    );
    assert_eq!(turn(1), (Some(Stop::EndTurn), None, 4096));
    let cut = left.exchange(1);
    assert_eq!(
        cut["answer"]["turn"]["parts"][0]["text"],
        "pass\nThe label rea"
    );
    assert_eq!(cut["answer"]["turn"]["stop"], "max_tokens");
}

/// One call, then `unsure`: the script of each transcript in which a call is refused.
fn one_call(name: &str, arguments: Json) -> String {
    let call = json!({"tool_call": {"id": "c1", "name": name, "arguments": arguments}});
    let script = json!({"script": "judge-script/1", "entries": [
        {"turn": {"parts": [call], "stop": "tool_use", "usage": null, "request_id": null}},
        {"turn": {"parts": [{"text": "unsure\nI could not look."}], "stop": "end_turn",
                  "usage": null, "request_id": null}},
    ]});
    script.to_string()
}

/// Runs a script of one call that is refused where it is dispatched: the refusal goes
/// back to the judge as the call's result and is in the run's record, no run was made
/// for the call, nothing is kept as evidence, and the run goes on to its verdict.
fn refused_at_dispatch(world: &World, name: &str, arguments: Json) -> (Left, String) {
    let (finished, left, judge) = run(world, &one_call(name, arguments), &[]);
    assert_eq!(finished.outcome().name(), "completed");
    let said = results(&judge, 1);
    assert!(
        said[0].is_error() && said[0].text().starts_with("refused: "),
        "{}",
        said[0].text()
    );
    let record = left.record();
    assert!(record.steps().is_empty(), "{:?}", record.steps());
    assert_eq!(record.outcome().refusals.len(), 1);
    assert_eq!(
        record.verdict(),
        Some((Decision::Unsure, "I could not look."))
    );
    assert!(!left.has("evidence-001"));
    let why = record.outcome().refusals[0].why().to_owned();
    (left, why)
}

#[test]
fn a_call_to_a_tool_that_is_not_on_the_list() {
    let world = World::new();
    let (left, why) = refused_at_dispatch(&world, "segment_everything", json!({}));
    let refused = refusals(left.record());
    assert_eq!(
        refused,
        [(Some("segment_everything"), RefusalKind::NotOffered)]
    );
    assert!(why.contains("froze the actions it calls"), "{why}");
}

#[test]
fn a_call_to_grow() {
    // `grow` is an action of this build, and the arguments fit it. It is not a tool of
    // the run: it is refused as any other is, and never runs.
    let world = World::new();
    let arguments = json!({"mask": world.photo.id().to_string(), "px": 8});
    let (left, _) = refused_at_dispatch(&world, "grow", arguments);
    assert_eq!(
        refusals(left.record()),
        [(Some("grow"), RefusalKind::NotOffered)]
    );
    assert_eq!(world.grown.load(Ordering::SeqCst), 0);
}

fn looking_at(image: ArtifactId, also: Option<ArtifactId>) -> Json {
    let mut arguments = json!({"image": image.to_string(), "bbox": [0.25, 0.25, 0.75, 0.75]});
    if let Some(also) = also {
        arguments["also"] = json!(also.to_string());
    }
    arguments
}

#[test]
fn view_on_an_image_the_run_was_not_given() {
    // The store holds the image. The run was not given it.
    let world = World::new();
    assert!(holds(&world.root(), world.fs.clone(), world.other.id()));
    let (left, why) = refused_at_dispatch(&world, "view", looking_at(world.other.id(), None));
    assert_eq!(
        refusals(left.record()),
        [(Some("view"), RefusalKind::NotReadable)]
    );
    assert!(
        why.contains("the input image is") && why.contains("was not given"),
        "{why}"
    );
}

#[test]
fn view_naming_a_second_image_the_run_was_not_given() {
    // The first image is the run's own; the second is not. Nothing is cut of either.
    let world = World::new();
    let arguments = looking_at(world.photo.id(), Some(world.other.id()));
    let (left, why) = refused_at_dispatch(&world, "view", arguments);
    assert_eq!(
        refusals(left.record()),
        [(Some("view"), RefusalKind::NotReadable)]
    );
    assert!(why.contains("the input also is"), "{why}");
}

#[test]
fn view_on_a_guessed_artifact_id() {
    // An identity that is nothing in the store is refused as one that is something:
    // the answer does not say which.
    let world = World::new();
    let guessed = ArtifactId(Digest::of_bytes(b"a guess"));
    assert!(!holds(&world.root(), world.fs.clone(), guessed));
    let (left, why) = refused_at_dispatch(&world, "view", looking_at(guessed, None));
    assert_eq!(
        refusals(left.record()),
        [(Some("view"), RefusalKind::NotReadable)]
    );
    let (_, held) = refused_at_dispatch(&world, "view", looking_at(world.other.id(), None));
    let same = held.replace(&world.other.id().to_string(), &guessed.to_string());
    assert_eq!(why, same);
}

const FAILS_ON_TURN_TWO: &str = r#"{"script":"judge-script/1","entries":[
  {"turn":{"parts":[{"tool_call":{"id":"c1","name":"view","arguments":{"image":"$photo","bbox":[0.25,0.25,0.75,0.75]}}}],
    "stop":"tool_use","usage":null,"request_id":null}},
  {"fault":{"kind":"unavailable","sentence":"overloaded","retry_after_ms":2000}}]}"#;

#[test]
fn a_failure_on_turn_two() {
    let world = World::new();
    let (finished, left, _) = run(&world, FAILS_ON_TURN_TWO, &[("photo", world.photo.id())]);
    let failed = Outcome::Error {
        kind: "judge_fault".to_owned(),
        message: "unavailable: overloaded".to_owned(),
    };
    assert_eq!(finished.outcome(), &failed);
    // A failed run: its terminal status, no verdict, no committed output and no
    // completed status, after the store was opened anew.
    let record = left.record();
    assert_eq!(record.outcome().status, Status::Error);
    assert_eq!(record.verdict(), None);
    commits_nothing(&left);
    // Its turn-one evidence is still readable, with every diagnostic written before
    // it ended: both exchanges and the journal, each as the record lists it.
    assert_eq!(left.evidence(1).id(), crop_of(&world.photo, HALF).id());
    assert_eq!(left.exchange(1)["answer"]["turn"]["stop"], "tool_use");
    assert_eq!(left.exchange(2)["answer"]["fault"]["kind"], "unavailable");
    let listed: Vec<_> = record.diagnostics().iter().map(|d| d.name()).collect();
    let names = ["evidence-001", "exchange-0001", "exchange-0002"];
    let mut expected: Vec<_> = names
        .iter()
        .map(|name| format!("diagnostics/{name}"))
        .collect();
    expected.push("journal".to_owned());
    assert_eq!(listed, expected);
    for diagnostic in record.diagnostics() {
        let (_, bytes) = &left.names[diagnostic.name()];
        assert_eq!(
            Digest::of_bytes(bytes),
            diagnostic.digest(),
            "{}",
            diagnostic.name()
        );
    }
    // The `view` run it called completed, and what that run committed remains.
    assert_eq!(steps(record), [("view", Some(Status::Completed))]);
    assert!(holds(
        &world.root(),
        world.fs.clone(),
        crop_of(&world.photo, HALF).id()
    ));
}

// --- evidence, and another run ----------------------------------------------------------------

#[test]
fn evidence_is_no_output_of_check_and_another_runs_judge_cannot_read_it() {
    let world = World::new();
    let crop = crop_of(&world.photo, HALF);
    let names = [("photo", world.photo.id()), ("crop", crop.id())];
    let (_, first, _) = run(&world, LOOK_CLOSER, &names);
    commits_nothing(&first);
    // The store holds the crop, as the output of the `view` run that cut it. A second
    // run of `check`, given the same photograph, is told the crop's identity by its
    // judge. It was not given that image and no call of its own returned it: refused.
    assert!(holds(&world.root(), world.fs.clone(), crop.id()));
    let (left, _) = refused_at_dispatch(&world, "view", looking_at(crop.id(), None));
    assert_eq!(
        refusals(left.record()),
        [(Some("view"), RefusalKind::NotReadable)]
    );
}

#[test]
fn known_a_crop_that_a_view_run_committed_is_an_artifact_a_caller_of_the_executor_may_take() {
    // What is not held, pinned as known (ADR 0009, "Evidence"): the crop is also the
    // committed output of the `view` run that cut it, and a caller of the executor,
    // which is no judge, takes an artifact by its identity as it takes any other.
    let world = World::new();
    let crop = crop_of(&world.photo, HALF);
    let names = [("photo", world.photo.id()), ("crop", crop.id())];
    run(&world, LOOK_CLOSER, &names);
    let executor = world.engine(judge(&one_call("view", json!({})), &[]), Secrets::none());
    let mut call = Call::new("view").input("image", crop.id());
    for (side, at) in ["left", "top", "right", "bottom"].into_iter().zip(CORNER) {
        call = call.argument(side, Value::Number(at));
    }
    let finished = executor.submit(&call).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
}

// --- the tool list after admission ------------------------------------------------------------

#[test]
fn changing_the_tool_list_after_admission_has_no_effect_on_the_run() {
    // The judge holds the run's first request open. While it does, `view` is
    // withdrawn. The run goes on with the tools it froze: its call of `view` is
    // dispatched. The next run sees the change: `view` is no tool on offer.
    let world = World::new();
    let barriers = Barriers::new();
    let names = [
        ("photo", world.photo.id()),
        ("crop", crop_of(&world.photo, HALF).id()),
    ];
    let script = LOOK_CLOSER.replace("$photo", &names[0].1.to_string());
    let script = script.replace("$crop", &names[1].1.to_string());
    let judge = FakeJudge::from_script(script.as_bytes()).unwrap();
    let judge = Arc::new(judge.stopping_at(&barriers, "judge"));
    let executor = world.engine(judge.clone(), Secrets::none());
    let Ok(Answer::Started(run)) = executor.start(&Request::new(world.check())) else {
        panic!("the run was not started");
    };
    barriers.arrived("judge", 1);
    world.toolbox.withdraw("view");
    assert!(world.toolbox.offered().is_empty());
    assert!(
        !world.toolbox.offer("grow"),
        "no tool of that name is linked"
    );
    barriers.open("judge");
    let watched = executor.wait(&run).unwrap();
    assert_eq!(
        watched.state(),
        &RunState::Ended(Outcome::Completed {
            outputs: Vec::new()
        })
    );
    let left = left(&world.root(), world.fs.clone(), &run);
    assert_eq!(steps(left.record()).len(), 2);
    assert_eq!(left.record().verdict().unwrap().0, Decision::Pass);
    // The tools the judge was offered were the same in every request.
    let offered: Vec<Vec<String>> = judge
        .received()
        .iter()
        .map(|request| {
            request
                .tools()
                .iter()
                .map(|tool| tool.name().to_owned())
                .collect()
        })
        .collect();
    assert_eq!(offered, vec![vec!["view".to_owned()]; 3]);
    // The next run is of the list as it is now.
    let next = executor.submit(&world.check()).unwrap();
    let invalid = Outcome::Invalid {
        why: "\"view\" is no evidence tool on offer".to_owned(),
    };
    assert_eq!(next.outcome(), &invalid);
    assert_eq!(judge.requests(), 3);
}

// --- publication, named for this action -------------------------------------------------------

#[test]
fn a_cancelled_check_has_no_verdict_and_keeps_its_evidence() {
    // The run is cancelled while the judge holds its second request, after the first
    // turn's call returned a crop. The cancellation is accepted.
    let world = World::new();
    let barriers = Barriers::new();
    let script = LOOK_CLOSER.replace("$photo", &world.photo.id().to_string());
    let judge = FakeJudge::from_script(script.as_bytes()).unwrap();
    let judge = Arc::new(judge.stopping_at(&barriers, "judge"));
    let executor = world.engine(judge.clone(), Secrets::none());
    let Ok(Answer::Started(run)) = executor.start(&Request::new(world.check())) else {
        panic!("the run was not started");
    };
    barriers.release("judge");
    barriers.arrived("judge", 2);
    assert_eq!(executor.cancel(&run), Ok(Cancel::Accepted));
    barriers.open("judge");
    assert_eq!(
        executor.wait(&run).unwrap().state(),
        &RunState::Ended(Outcome::Cancelled)
    );
    drop(executor);

    // The store is closed and opened anew: the run has its terminal status, no
    // verdict, no committed output and no completed status; and every diagnostic
    // written before it ended is readable, the evidence among them.
    let left = left(&world.root(), world.fs.clone(), &run);
    let record = left.record();
    assert_eq!(record.outcome().status, Status::Cancelled);
    assert_eq!(record.verdict(), None);
    assert!(
        record
            .judge()
            .iter()
            .all(|entry| matches!(entry, Judged::Turn { .. }))
    );
    commits_nothing(&left);
    assert_eq!(left.evidence(1).id(), crop_of(&world.photo, HALF).id());
    assert_eq!(left.exchange(1)["answer"]["turn"]["stop"], "tool_use");
    assert_eq!(left.exchange(2)["answer"]["fault"]["kind"], "cancelled");
    assert_eq!(record.diagnostics().len(), 4);
}

const DECIDES: &str = r#"{"script":"judge-script/1","entries":[
  {"turn":{"parts":[{"text":"pass\nNothing is wrong."}],"stop":"end_turn","usage":null,"request_id":null}}]}"#;

#[test]
fn a_completed_check_was_reported_only_once_its_verdict_and_status_were_durable() {
    // Power is lost the moment the run was reported: everything not yet durable
    // goes. The store, opened anew on another disk handle, has the completed status
    // and the verdict, both.
    let world = World::new();
    let executor = world.engine(judge(DECIDES, &[]), Secrets::none());
    let finished = executor.submit(&world.check()).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    world.fs.crash(Lost::Unsynced);
    drop(executor);
    let fs: Arc<dyn Fs> = Arc::new(ShimFs::new());
    let left = left(&world.root(), fs, finished.run());
    assert_eq!(left.record().outcome().status, Status::Completed);
    assert_eq!(
        left.record().verdict(),
        Some((Decision::Pass, "Nothing is wrong."))
    );
}

/// A judge that notes how many calls had changed the disk when it was last asked.
struct Marking(Arc<FakeJudge>, Arc<ShimFs>, std::sync::Mutex<usize>);

impl Judge for Marking {
    fn declaration(&self) -> &Declaration {
        self.0.declaration()
    }

    fn send(&self, request: &JudgeRequest, bounds: &Bounds) -> Result<Turn, Fault> {
        *self.2.lock().unwrap() = self.1.events().len();
        self.0.send(request, bounds)
    }
}

#[test]
fn a_check_whose_end_the_store_refuses_at_any_step_is_not_reported_as_completed() {
    // The calls of the run's end publication, counted on a run that nothing stops:
    // those from the judge's last answer on. Then the store refuses each in turn. The
    // answer is the store's failure and never a completed run; and what the store
    // then holds is the run as it was admitted, with no status and no verdict, or,
    // where the end became visible and its last sync failed, the status and the
    // verdict together.
    let count = {
        let world = World::new();
        let marking = Arc::new(Marking(judge(DECIDES, &[]), world.fs.clone(), 0.into()));
        let executor = world.engine(marking.clone(), Secrets::none());
        executor.submit(&world.check()).unwrap();
        let start = *marking.2.lock().unwrap();
        (start, world.fs.events().len() - start)
    };
    let (start, steps) = count;
    assert!((5..40).contains(&steps), "{steps} calls");
    let mut seen = std::collections::BTreeSet::new();
    for step in 0..steps {
        let world = World::new();
        let executor = world.engine(judge(DECIDES, &[]), Secrets::none());
        world.fs.refuse_before(start + step);
        let answer = executor.submit(&world.check());
        drop(executor);
        let reader = whizbang_core::store::Reader::open(&world.root(), world.fs.clone()).unwrap();
        let generation = reader.snapshot().unwrap();
        let run = RunId::parse(generation.runs().keys().next().unwrap()).unwrap();
        let left = left(&world.root(), world.fs.clone(), &run);
        match answer {
            Err(CallError::Store(_)) => {
                let both = left.record.as_ref().is_some_and(|record| {
                    record.outcome().status == Status::Completed && record.verdict().is_some()
                });
                assert_eq!(both, left.record.is_some(), "step {step}");
                assert_eq!(left.names.len() == 1, !both, "step {step}");
                seen.insert(both);
            }
            // Clearing the staging away, after the end is durable, can fail without
            // failing the run.
            Ok(finished) => {
                assert_eq!(step, steps - 1);
                assert_eq!(finished.outcome().name(), "completed");
                assert!(left.record().verdict().is_some());
            }
            other => panic!("step {step}: {other:?}"),
        }
    }
    assert_eq!(seen.into_iter().collect::<Vec<_>>(), [false, true]);
}

// --- a judge that is not trusted ----------------------------------------------------------------

/// A script of these turns, each a list of parts and the word it stopped with.
fn turns(turns: &[(Json, &str)]) -> String {
    let entry = |(parts, stop): &(Json, &str)| json!({"turn": {"parts": parts, "stop": stop, "usage": null, "request_id": null}});
    let entries: Vec<Json> = turns.iter().map(entry).collect();
    json!({"script": "judge-script/1", "entries": entries}).to_string()
}

fn calls(id: &str, world: &World) -> Json {
    json!({"tool_call": {"id": id, "name": "view", "arguments": looking_at(world.photo.id(), None)}})
}

fn error_of(world: &World, script: &str) -> (String, String, Left) {
    let (finished, left, _) = run(world, script, &[]);
    match finished.outcome() {
        Outcome::Error { kind, message } => (kind.clone(), message.clone(), left),
        other => panic!("{other:?}"),
    }
}

#[test]
fn what_a_judge_that_is_not_trusted_answers_is_a_typed_outcome_and_never_a_panic() {
    let world = World::new();
    let output = |script: &str, says: &str| {
        let (kind, message, left) = error_of(&world, script);
        assert_eq!(kind, "judge_output", "{message}");
        assert!(message.contains(says), "{message}");
        assert_eq!(left.record().verdict(), None);
        left
    };
    // Cut off again after the one retry: both turns are marked as left out.
    let cut = (json!([{"text": "pa"}]), "max_tokens");
    let left = output(&turns(&[cut.clone(), cut.clone()]), "cut off again");
    assert_eq!(left.record().judge().len(), 2);
    // An empty turn that was cut off is a turn; an empty script is the fake's fault.
    let done = (json!([{"text": "pass"}]), "end_turn");
    let (finished, _, _) = run(
        &world,
        &turns(&[(json!([]), "max_tokens"), done.clone()]),
        &[],
    );
    assert_eq!(finished.outcome().name(), "completed");
    let (kind, message, _) = error_of(&world, &turns(&[]));
    assert_eq!(
        (kind.as_str(), message.as_str()),
        (
            "judge_fault",
            "malformed: the scripted transcript has no turn left"
        )
    );
    // No verdict in the last turn; a turn that ends with calls unanswered; a reason
    // the loop does not know; a refusal.
    output(
        &turns(&[(json!([{"text": "It looks fine to me."}]), "end_turn")]),
        "without pass, fail or unsure",
    );
    output(
        &turns(&[(json!([calls("c1", &world)]), "end_turn")]),
        "tool calls unanswered",
    );
    let other = json!({"script": "judge-script/1", "entries": [{"turn": {
        "parts": [{"text": "pass"}], "stop": {"other": "pause_turn"}, "usage": null, "request_id": null}}]});
    output(&other.to_string(), "a reason this loop does not know");
    let (kind, _, left) = error_of(&world, &turns(&[(json!([]), "refusal")]));
    assert_eq!(kind, "judge_refused");
    let Judged::Turn { excluded, .. } = &left.record().judge()[0] else {
        panic!("a turn");
    };
    assert_eq!(*excluded, Some(Excluded::Refusal));
    // A call whose id an earlier call had: the first was answered, the second is not.
    let again = [
        (json!([calls("c1", &world)]), "tool_use"),
        (json!([calls("c1", &world)]), "tool_use"),
    ];
    let left = output(&turns(&again), "the id of a call that was answered before");
    assert_eq!(steps(left.record()).len(), 1);
    // A turn with more text than the loop takes.
    let long = "x".repeat(TURN_TEXT_LIMIT + 1);
    output(
        &turns(&[(json!([{"text": long}]), "end_turn")]),
        "bytes of text",
    );
}

#[test]
fn a_judge_that_never_concludes_is_stopped_at_the_loops_bounds() {
    // Every turn calls `view` three times. The loop sends as many requests as it may
    // and no more, and dispatches as many calls as it may: the rest are answered
    // with an error and make no run.
    let world = World::new();
    let turn = |at: usize| {
        let ids = [0, 1, 2].map(|n| calls(&format!("c{at}-{n}"), &world));
        (json!(ids), "tool_use")
    };
    let script: Vec<_> = (0..TURNS_LIMIT + 2).map(turn).collect();
    let (finished, left, judge) = run(&world, &turns(&script), &[]);
    let Outcome::Error { kind, message } = finished.outcome() else {
        panic!("{:?}", finished.outcome());
    };
    assert_eq!(kind, "judge_loop");
    assert_eq!(message, "the judge did not conclude within 16 requests");
    assert_eq!(judge.requests() as usize, TURNS_LIMIT);
    assert_eq!(judge.remaining(), 2);
    assert_eq!(left.record().steps().len(), CALLS_LIMIT);
    assert!(left.has("evidence-032") && !left.has("evidence-033"));
    let last = results(&judge, TURNS_LIMIT - 1);
    assert_eq!(last[2].text(), "not called: a run calls at most 32 tools");
}

#[test]
fn a_secret_in_a_judges_turn_a_tool_argument_and_a_tool_error_is_in_nothing_under_the_store() {
    // A made-up key, put together here: in the judge's text, in an argument of a call
    // the tool rejects (so that the error is about it), in the name of a tool that is
    // not on the list, in an opaque part, and in the reason of the verdict.
    let key = format!("sk{}squintcheck0123456789ABCDEFGHIJ", "-");
    let world = World::new();
    let parts = json!([
        {"text": format!("The sticker reads {key}.")},
        {"tool_call": {"id": "c1", "name": "view", "arguments": {"image": key, "bbox": [0, 0, 1, 1]}}},
        {"tool_call": {"id": "c2", "name": key, "arguments": {"token": key}}},
        {"tool_call": {"id": "c3", "name": "view",
                       "arguments": {"image": world.photo.id().to_string(), "bbox": [0, 0, 1, 1], key.as_str(): 1}}},
        {"opaque": {"adapter": "fake", "body": format!("thought {key}")}},
    ]);
    let verdict = json!([{"text": format!("fail\nIt shows {key}.")}]);
    let script = turns(&[(parts, "tool_use"), (verdict, "end_turn")]);
    let judge = judge(&script, &[]);
    let secrets = Secrets::new([key.clone()]).unwrap();
    let executor = world.engine(judge.clone(), secrets);
    let finished = executor.submit(&world.check()).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    drop(executor);
    // The judge was sent its own turn back whole, key and all.
    let said_back = format!("{:?}", judge.received()[1].messages()[1]);
    let whole = format!("Opaque(fake, {} bytes)", "thought ".len() + key.len());
    assert!(said_back.contains(&whole), "{said_back}");
    let files = files_under(&world.root());
    assert!(files.len() > 8);
    for (path, bytes) in &files {
        let found = bytes
            .windows(key.len())
            .any(|window| window == key.as_bytes());
        assert!(!found, "{path:?} holds the key");
    }
    let left = left(&world.root(), world.fs.clone(), finished.run());
    let record = left.record();
    assert_eq!(
        record.verdict(),
        Some((Decision::Fail, "It shows [redacted]."))
    );
    // The call of a tool named by the key is recorded with fixed words and no name.
    let refused = &record.outcome().refusals[0];
    assert_eq!(
        (refused.action(), refused.kind()),
        (None, RefusalKind::NotOffered)
    );
    assert!(refused.why().contains("is not quoted"), "{}", refused.why());
}
