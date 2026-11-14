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

#[test]
fn arguments_that_are_not_a_views_make_no_call_and_a_box_past_the_image_is_refused() {
    // Five calls in one turn. A box whose entries are not all numbers is no box,
    // however many of them are: nothing is called. Nor is anything called for a key
    // that is no argument of the tool. A box that reaches past the image is a call,
    // which the executor refuses when it prepares it: no run is made, and the
    // refusal is in the record. Each answer goes back as its call's result.
    let world = World::new();
    let photo = world.photo.id().to_string();
    let call = |id: &str, arguments: Json| json!({"tool_call": {"id": id, "name": "view", "arguments": arguments}});
    let parts = json!([
        call(
            "c1",
            json!({"image": photo, "bbox": [0, "x", 0.25, 0.75, 0.75]})
        ),
        call(
            "c2",
            json!({"image": photo, "bbox": [0.25, null, 0.75, 0.75]})
        ),
        call(
            "c3",
            json!({"image": photo, "bbox": [0, 0, 0.5, 0.5, 0.75]})
        ),
        call(
            "c4",
            json!({"image": photo, "bbox": [0, 0, 0.5, 0.5], "zoom": 2})
        ),
        call(
            "c5",
            json!({"image": photo, "bbox": [0.25, 0.25, 0.75, 1.5]})
        ),
    ]);
    let done = (json!([{"text": "unsure"}]), "end_turn");
    let (finished, left, judge) = run(&world, &turns(&[(parts, "tool_use"), done]), &[]);
    assert_eq!(finished.outcome().name(), "completed");
    let said = results(&judge, 1);
    assert!(
        said.iter()
            .all(|result| result.is_error() && result.images().is_empty())
    );
    let said: Vec<&str> = said.iter().map(ToolResult::text).collect();
    let no_box = "not called: bbox is four numbers: left, top, right and bottom";
    assert_eq!(said[..3], [no_box, no_box, no_box]);
    assert_eq!(
        said[3],
        "not called: the arguments are image, bbox and also, and no other"
    );
    assert_eq!(
        said[4],
        "refused: not a call of view: the argument bottom: 1.5 is outside 0 to 1"
    );
    let record = left.record();
    assert!(record.steps().is_empty(), "{:?}", record.steps());
    assert_eq!(
        refusals(record),
        [(Some("view"), RefusalKind::Invalid)],
        "the box past the image, and no other"
    );
    assert_eq!(runs(&world.root(), world.fs.clone()), 1);
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

#[test]
fn an_action_that_runs_with_no_arguments_and_is_not_on_the_list_is_refused_before_any_call() {
    // The stand-in for `grow` is registered and is a valid call with nothing in it.
    // The judge names it with no arguments. The loop refuses the name against the
    // list it froze, before it hands the executor anything: the action never runs,
    // no run is made for it, nothing is dispatched, and the refusal is in the record.
    let world = World::new();
    let (left, why) = refused_at_dispatch(&world, "grow", json!({}));
    assert_eq!(world.grown.load(Ordering::SeqCst), 0);
    assert_eq!(
        refusals(left.record()),
        [(Some("grow"), RefusalKind::NotOffered)]
    );
    assert!(why.contains("froze the actions it calls"), "{why}");
    // The store holds the run of `check` and no other.
    assert_eq!(runs(&world.root(), world.fs.clone()), 1);
    // The loop reports each call it hands the executor. It handed over none.
    let journal = String::from_utf8(left.names["journal"].1.clone()).unwrap();
    assert!(!journal.contains("dispatched"), "{journal}");
    // A call of a listed tool is reported, so the line above can fail.
    let (_, looked, _) = run(&world, LOOK_CLOSER, &[("photo", world.photo.id())]);
    let journal = String::from_utf8(looked.names["journal"].1.clone()).unwrap();
    assert!(journal.contains("a tool call was dispatched"), "{journal}");
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
fn a_check_whose_end_is_refused_at_any_step_is_not_reported_as_completed_a_sweep_like_a_crash_at_any_event()
 {
    // (The name ends as it does so that this sweep runs one at a time with the crash
    // sweeps, by the filter of `.config/nextest.toml`: it runs a store once for each
    // step, and is slow beside them. Nothing here crashes.)
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

#[test]
fn a_failure_of_the_store_inside_a_running_view_or_check_leaves_no_path_in_its_record() {
    // The photograph's file goes missing after the call was admitted and before the
    // action reads it. Both actions copy the artifact's error whole into their
    // failure; the store in their context said it without a path, so neither wrote
    // one (`grow` has the same test in its crate).
    for (damaged, kind) in [("view", "view"), ("check", "image")] {
        let world = World::new();
        let root = world.root();
        let file = whizbang_core::artifact::Artifact::Image(world.photo.clone()).to_bytes();
        let file = whizbang_core::store::Digest::of_bytes(&file).hex();
        let gone = root.join("objects").join(&file);
        assert!(gone.exists());
        let wrap = |action: Arc<dyn whizbang_core::contract::Action>| {
            if action.descriptor().name() != damaged {
                return action;
            }
            let gone = gone.clone();
            let remove = move || drop(std::fs::remove_file(&gone));
            Arc::new(Before(action, Box::new(remove))) as Arc<dyn whizbang_core::contract::Action>
        };
        let judge = judge(LOOK_CLOSER, &[("photo", world.photo.id())]);
        let executor = world.engine_wrapping(judge, Secrets::none(), wrap);
        let call = match damaged {
            "check" => world.check(),
            _ => Call::new("view")
                .input("image", world.photo.id())
                .argument("left", Value::Number(0.25))
                .argument("top", Value::Number(0.25))
                .argument("right", Value::Number(0.75))
                .argument("bottom", Value::Number(0.75)),
        };
        let finished = executor.submit(&call).unwrap();
        drop(executor);
        let Outcome::Error {
            kind: said,
            message,
        } = finished.outcome()
        else {
            panic!("{damaged}: {:?}", finished.outcome());
        };
        assert_eq!(said, kind);
        assert_eq!(
            message,
            &format!("the store's file {file} is missing or damaged"),
            "{damaged}"
        );
        let left = left(&root, world.fs.clone(), finished.run());
        let folder = root.parent().unwrap().file_name().unwrap();
        for (name, (_, bytes)) in &left.names {
            let text = String::from_utf8_lossy(bytes);
            assert!(!text.contains(root.to_str().unwrap()), "{name}: {text}");
            assert!(
                !text.contains(folder.to_str().unwrap()) && !text.contains("objects"),
                "{damaged}: {name}: {text}"
            );
        }
    }
}

#[test]
fn evidence_that_holds_a_secrets_bytes_is_not_kept_altered_and_the_run_fails() {
    // Evidence is an image's file. A secret the executor was told of can stand in
    // such a file by chance, and taking it out would leave a file that is no image
    // under the evidence's name. So evidence is written exactly: here the crop's
    // file holds the secret's bytes, the evidence is not kept, and the run fails.
    // The crop itself is the committed output of the `view` run, which stores an
    // artifact as its bytes, as every artifact is stored.
    let world = World::new();
    let crop = crop_of(&world.photo, HALF);
    let file = whizbang_core::artifact::Artifact::Image(crop.clone()).to_bytes();
    let space = |byte: &u8| byte.is_ascii_whitespace();
    let stands_in_it = file.windows(8).find_map(|window| {
        let text = std::str::from_utf8(window).ok()?;
        (!space(&window[0]) && !space(&window[7])).then(|| text.to_owned())
    });
    let secret = stands_in_it.expect("eight bytes of the file that are a text");
    let names = [("photo", world.photo.id()), ("crop", crop.id())];
    let judge = judge(LOOK_CLOSER, &names);
    let secrets = Secrets::new([secret.clone()]).unwrap();
    let executor = world.engine(judge, secrets);
    let finished = executor.submit(&world.check()).unwrap();
    drop(executor);
    assert_eq!(
        finished.outcome(),
        &Outcome::Error {
            kind: "evidence".to_owned(),
            message: "the bytes hold something that must not be written, and they are a \
                      file that cannot be changed: nothing was kept"
                .to_owned(),
        }
    );
    let left = left(&world.root(), world.fs.clone(), finished.run());
    assert!(!left.has("evidence-001"));
    assert_eq!(steps(left.record()), [("view", Some(Status::Completed))]);
    // The crop the tool's run committed is whole.
    let reader = whizbang_core::store::Reader::open(&world.root(), world.fs.clone()).unwrap();
    let kind = whizbang_core::artifact::ArtifactType::Image;
    let held = whizbang_core::artifact::get(&reader, kind, crop.id(), &limits()).unwrap();
    assert_eq!(held.to_bytes(), file);
}

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
    // A turn with more text than the loop takes is not taken by its handle: a fault,
    // and what is stored of the turn is its sizes.
    let long = "x".repeat(TURN_TEXT_LIMIT + 1);
    let script = turns(&[(json!([{"text": long}]), "end_turn")]);
    let (kind, message, left) = error_of(&world, &script);
    assert_eq!(kind, "judge_fault");
    assert!(
        message.starts_with("too_large: the turn holds 262145 bytes of text"),
        "{message}"
    );
    let kept = &left.exchange(1)["answer"]["turn_withheld"];
    assert_eq!(kept["bytes"].as_u64(), Some(262_145));
    assert!(left.diagnostic("exchange-0001").len() < 4000);
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

// --- the record, against the spec's list ------------------------------------------------------

/// Runs `check` on the photograph against the look-closer script, through an executor
/// with a ledger whose judge states a bound of 250, under the request id `exit-1`; the
/// run ends as `how` says: it completes, its judge fails on the second turn, or it is
/// cancelled while the judge holds its second request.
fn ended(world: &World, how: &str, ledger: Arc<whizbang_testkit::ledger::FakeLedger>) -> RunId {
    let crop = crop_of(&world.photo, HALF);
    let script = if how == "error" {
        FAILS_ON_TURN_TWO
    } else {
        LOOK_CLOSER
    };
    let script = script
        .replace("$photo", &world.photo.id().to_string())
        .replace("$crop", &crop.id().to_string());
    let barriers = Barriers::new();
    let judge = FakeJudge::from_script(script.as_bytes()).unwrap();
    let judge: Arc<dyn Judge> = match how {
        "cancelled" => Arc::new(judge.stopping_at(&barriers, "judge")),
        _ => Arc::new(judge),
    };
    let priced = world.engine_priced(judge, Secrets::none(), |action| action, 250);
    let executor = priced.with_ledger(ledger);
    let id = whizbang_core::contract::RequestId::parse("exit-1").unwrap();
    let request = Request::new(world.check()).with_id(id);
    if how != "cancelled" {
        let Ok(Answer::Ran(finished)) = executor.request(&request) else {
            panic!("{how}: the request did not run");
        };
        assert_eq!(finished.outcome().name(), how);
        return finished.run().clone();
    }
    let Ok(Answer::Started(run)) = executor.start(&request) else {
        panic!("the run was not started");
    };
    barriers.release("judge");
    barriers.arrived("judge", 2);
    assert_eq!(executor.cancel(&run), Ok(Cancel::Accepted));
    barriers.open("judge");
    let waited = executor.wait(&run).unwrap();
    assert_eq!(waited.state(), &RunState::Ended(Outcome::Cancelled));
    run
}

#[test]
fn the_records_of_check_completed_failed_and_cancelled_hold_what_the_spec_lists() {
    // `SPEC.md` section 3, "What every run records", group by group, for a run of
    // `check` that completed, one whose judge failed on its second turn and one
    // that was cancelled, each through an executor with a ledger and a judge that
    // states a bound. Egress is there and empty: nothing in this checkpoint sends
    // data anywhere. A model's device and whether a cache answered are null: no
    // response says either yet.
    for (how, turns, calls) in [("completed", 3, 2), ("error", 2, 1), ("cancelled", 2, 1)] {
        let world = World::new();
        let ledger = Arc::new(whizbang_testkit::ledger::FakeLedger::new());
        let run = ended(&world, how, ledger.clone());
        let left = left(&world.root(), world.fs.clone(), &run);
        let record: Json = serde_json::from_slice(&left.names["record"].1).unwrap();

        // Identity.
        assert_eq!(
            record["identity"],
            json!({"run": run.as_str(), "request": "exit-1", "parent": null,
                   "whizbang": env!("CARGO_PKG_VERSION")}),
            "{how}"
        );
        // What was asked: the action, and its exact arguments, the two that have a
        // value by default among them; no settings yet; the config's digest.
        assert_eq!(
            record["asked"],
            json!({"action": "check", "version": "1.0.0", "settings": [],
                   "arguments": [{"name": "tools", "value": {"text": "view"}},
                                 {"name": "max_output_tokens", "value": {"integer": 1024}}],
                   "config": Digest::of_bytes(b"").to_string()}),
            "{how}"
        );
        // Inputs and outputs: the photograph with its binding. `check` commits no
        // output, whatever it comes to: its result is its verdict.
        assert_eq!(
            record["inputs"],
            json!([{"port": "images", "type": "image",
                    "artifact": world.photo.id().to_string(),
                    "binding": world.photo.binding().id().to_string()}]),
            "{how}"
        );
        assert_eq!(record["outputs"], json!([]), "{how}");
        // Outcome: the status, and for the error its type and message.
        let error = match how {
            "error" => json!({"kind": "judge_fault", "message": "unavailable: overloaded"}),
            _ => Json::Null,
        };
        assert_eq!(
            record["outcome"],
            json!({"status": how, "error": error, "invalid": null, "refusals": [],
                   "refusals_omitted": 0}),
            "{how}"
        );

        // Models: one for every request to the judge, with the fake's identity.
        let models = record["models"].as_array().unwrap();
        assert_eq!(models.len(), turns, "{how}");
        for (at, model) in models.iter().enumerate() {
            let ended = match (how, at) {
                ("error", 1) => "unavailable",
                ("cancelled", 1) => "cancelled",
                _ => "answered",
            };
            assert_eq!(
                model,
                &json!({"capability": "judge", "backend": "fake", "model": "fake-judge",
                        "version": "1", "device": null, "cached": null, "ended": ended}),
                "{how}: request {at}"
            );
        }
        // Steps: every tool call, in order, with the run it made; and each of those
        // runs has a record of its own, under this run, with the call's arguments.
        let steps = record["steps"].as_array().unwrap();
        assert_eq!(steps.len(), calls, "{how}");
        for step in steps {
            assert_eq!(step["step"], "call");
            assert_eq!(
                (&step["action"], &step["status"]),
                (&json!("view"), &json!("completed"))
            );
            let child = RunId::parse(step["run"].as_str().unwrap()).unwrap();
            let child = super::world::left(&world.root(), world.fs.clone(), &child);
            let child: Json = serde_json::from_slice(&child.names["record"].1).unwrap();
            assert_eq!(child["identity"]["parent"], run.as_str());
            assert_eq!(child["asked"]["arguments"].as_array().unwrap().len(), 4);
            assert_eq!(child["outputs"].as_array().unwrap().len(), 1);
            // What it drew from this run's allowance: `view` costs nothing.
            assert_eq!(child["spend"]["reserved_micros"], 0);
        }
        // Time: start and end, and a duration for each request and each call.
        let time = &record["time"];
        assert!(time["start_ms"].as_u64() < time["end_ms"].as_u64(), "{how}");
        let durations = time["durations"].as_array().unwrap();
        let of = |what: &str| durations.iter().filter(|d| d["of"] == what).count();
        assert_eq!((of("request"), of("call")), (turns, calls), "{how}");
        assert_eq!(durations.len(), turns + calls);
        // Spend: what the ledger reserved for the run, the judge's bound, and no
        // paid request, since no fake charges. The reservation is in the store.
        assert_eq!(
            record["spend"],
            json!({"reserved_micros": 250, "requests": []}),
            "{how}"
        );
        let reader = whizbang_core::store::Reader::open(&world.root(), world.fs.clone());
        let reserved = ledger.reservations(&reader.unwrap()).unwrap();
        let [reservation] = reserved.as_slice() else {
            panic!("{how}: {} reservations", reserved.len());
        };
        assert_eq!((reservation.run(), reservation.micros()), (&run, 250));
        assert_eq!(record["egress"], json!([]), "{how}");

        // Judge: every turn, with what was sent and received kept whole in the
        // exchange it names; token usage; and the verdict, of the run that
        // completed and of no other.
        let judged = record["judge"].as_array().unwrap();
        let verdict = json!({"judge": "verdict", "decision": "pass",
                             "why": "The corner is clean."});
        match how {
            "completed" => assert_eq!(judged.last(), Some(&verdict)),
            _ => assert!(judged.iter().all(|entry| entry["judge"] == "turn"), "{how}"),
        }
        let turned: Vec<&Json> = judged.iter().filter(|e| e["judge"] == "turn").collect();
        assert_eq!(turned.len(), turns, "{how}");
        let usage = json!({"input_tokens": 900, "output_tokens": 40,
                           "cache_read_tokens": 0, "cache_write_tokens": 0});
        // The script of the failing judge states no usage and no id of its own.
        if how != "error" {
            assert_eq!(turned[0]["usage"], usage, "{how}");
            assert_eq!(turned[0]["provider_id"], "r-1");
        }
        for (at, turn) in turned.iter().enumerate() {
            assert_eq!(turn["request"], at);
            assert_eq!(turn["excluded"], Json::Null);
            let exchange = left.exchange(at + 1);
            assert_eq!(
                turn["exchange"],
                format!("diagnostics/exchange-{:04}", at + 1)
            );
            // What was sent: the system prompt, every message so far, the tools and
            // the limit. A picture is its size and its pixel digest.
            assert!(
                exchange["system"]
                    .as_str()
                    .unwrap()
                    .starts_with("You check images.")
            );
            let sent = exchange["messages"].as_array().unwrap();
            assert_eq!(turn["messages"], sent.len());
            assert_eq!(sent.len(), 1 + 2 * at, "{how}: turn {at}");
            let pixels = world.photo.identity().pixels.to_string();
            let shown = json!({"image": {"width": 96, "height": 64, "pixels": pixels}});
            assert_eq!(sent[0]["parts"][1], shown);
            assert_eq!(exchange["tools"][0]["name"], "view");
            assert_eq!(exchange["max_output_tokens"], 1024);
            // What was received: the turn as the judge gave it, or its fault.
            let answer = &exchange["answer"];
            match (how, at) {
                ("error", 1) => assert_eq!(answer["fault"]["kind"], "unavailable"),
                ("cancelled", 1) => assert_eq!(answer["fault"]["kind"], "cancelled"),
                _ => {
                    assert_eq!(turn["stop"], answer["turn"]["stop"]);
                    let calls = answer["turn"]["parts"].as_array().unwrap();
                    let calls = calls.iter().filter(|p| p.get("tool_call").is_some());
                    assert_eq!(turn["tool_calls"], calls.count());
                }
            }
            // A turn that was answered is said back whole in the next request.
            if at > 0 {
                let before = left.exchange(at);
                assert_eq!(sent[2 * at - 1]["parts"], before["answer"]["turn"]["parts"]);
                assert_eq!(sent[2 * at - 1]["role"], "assistant");
                assert_eq!(
                    sent[..2 * at - 1],
                    before["messages"].as_array().unwrap()[..]
                );
            }
        }
        let first = left.exchange(1);
        let said = &first["answer"]["turn"]["parts"];
        if how != "error" {
            assert_eq!(said[0], json!({"text": "I will look at the middle."}));
        }
        let looked = json!({"image": world.photo.id().to_string(),
                            "bbox": [0.25, 0.25, 0.75, 0.75]});
        let call = said.as_array().unwrap().last().unwrap();
        assert_eq!(call["tool_call"]["arguments"], looked);

        // Diagnostics: the journal, each exchange and each crop kept as evidence,
        // every one listed with the digest and length of what the store holds.
        let listed = record["diagnostics"].as_array().unwrap();
        assert_eq!(listed.len(), 1 + turns + calls, "{how}");
        for diagnostic in listed {
            let name = diagnostic["name"].as_str().unwrap();
            let (kind, bytes) = &left.names[name];
            assert_eq!(*kind, Stored::Diagnostic);
            assert_eq!(diagnostic["bytes"].as_u64(), Some(bytes.len() as u64));
            let digest = whizbang_core::store::Digest::of_bytes(bytes);
            assert_eq!(diagnostic["digest"], format!("sha256:{}", digest.hex()));
        }
        assert!(left.has("evidence-001") && left.names.contains_key("journal"));
        assert_eq!(record["record"], "run/2");
        assert_eq!(record.as_object().unwrap().len(), 13, "{how}");
    }
}
