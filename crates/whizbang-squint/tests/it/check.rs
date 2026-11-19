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

const CUT_OFF_THEN_A_CALL: &str = r#"{"script":"judge-script/1","entries":[
  {"turn":{"parts":[{"text":"pass\nThe label rea"}],"stop":"max_tokens","usage":null,"request_id":null}},
  {"turn":{"parts":[{"tool_call":{"id":"c1","name":"view","arguments":{"image":"$photo","bbox":[0,0,1,1]}}}],
    "stop":"tool_use","usage":null,"request_id":null}},
  {"turn":{"parts":[{"text":"fail\nThe label reads wrong."}],"stop":"end_turn","usage":null,"request_id":null}}]}"#;

#[test]
fn after_a_cut_off_turn_the_raised_limit_holds_for_the_rest_of_the_run() {
    // E1's live run (plan 2C, task F1): a judge that wrote past its limit once wrote
    // past it again on a later turn. The turn after the retry keeps the raised limit.
    let world = World::new();
    let (finished, _, judge) = run(&world, CUT_OFF_THEN_A_CALL, &[("photo", world.photo.id())]);
    assert_eq!(finished.outcome().name(), "completed");
    let limits: Vec<u32> = judge
        .received()
        .iter()
        .map(|request| request.controls().max_output_tokens())
        .collect();
    assert_eq!(limits, [1024, 1024 * RETRY_FACTOR, 1024 * RETRY_FACTOR]);
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

/// The second request ends in a fault that is final: one the judge's backend could not
/// answer now would be sent again.
const FAILS_ON_TURN_TWO: &str = r#"{"script":"judge-script/1","entries":[
  {"turn":{"parts":[{"tool_call":{"id":"c1","name":"view","arguments":{"image":"$photo","bbox":[0.25,0.25,0.75,0.75]}}}],
    "stop":"tool_use","usage":null,"request_id":null}},
  {"fault":{"kind":"rejected","sentence":"refused","retry_after_ms":null}}]}"#;

#[test]
fn a_failure_on_turn_two() {
    let world = World::new();
    let (finished, left, _) = run(&world, FAILS_ON_TURN_TWO, &[("photo", world.photo.id())]);
    let failed = Outcome::Error {
        kind: "judge_fault".to_owned(),
        message: "rejected: refused".to_owned(),
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
    assert_eq!(left.exchange(2)["answer"]["fault"]["kind"], "rejected");
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
    assert_eq!(
        world.toolbox.offered(),
        ["align", "change_map", "edge_similarity", "resize"]
    );
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
pub fn turns(turns: &[(Json, &str)]) -> String {
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

#[test]
fn a_call_of_check_may_cost_the_bound_of_one_request_to_its_judge_for_every_turn_of_its_loop() {
    // The need for a judge states the most requests one run sends it (ADR 0010), so
    // what a call is prepared with, and reserves, covers every turn; `view`, which
    // `check` calls, needs no backend and may cost nothing.
    let world = World::new();
    let executor = world.engine_priced(judge(LOOK_CLOSER, &[]), Secrets::none(), |a| a, 250);
    let prepared = executor.prepare(&world.check()).unwrap();
    assert_eq!(prepared.estimate().micros(), 250 * TURNS_LIMIT as u64);
    let view = Call::new("view")
        .input("image", world.photo.id())
        .argument("bbox", Value::text("0,0,1,1"));
    if let Ok(prepared) = executor.prepare(&view) {
        assert_eq!(prepared.estimate().micros(), 0);
    }
}

/// Runs `check` on the photograph against the look-closer script, through an executor
/// with a ledger whose judge states a bound of 250 for a request, under the request id `exit-1`; the
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
        // What was asked: the action, and its exact arguments, the seven that have a
        // value by default among them; no settings yet; the config's digest.
        assert_eq!(
            record["asked"],
            json!({"action": "check", "version": "1.5.0", "settings": [],
                   "arguments": [{"name": "tools", "value": {"text": "view"}},
                                 {"name": "use_case", "value": {"text": "general"}},
                                 {"name": "details", "value": {"text": "none"}},
                                 {"name": "detail", "value": {"text": "align"}},
                                 {"name": "projection", "value": {"text": "full"}},
                                 {"name": "second_look", "value": {"text": "off"}},
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
            "error" => json!({"kind": "judge_fault", "message": "rejected: refused"}),
            _ => Json::Null,
        };
        assert_eq!(
            record["outcome"],
            json!({"status": how, "error": error, "invalid": null, "refusals": [],
                   "refusals_omitted": 0, "lost": []}),
            "{how}"
        );

        // Models: one for every request to the judge, with the fake's identity.
        let models = record["models"].as_array().unwrap();
        assert_eq!(models.len(), turns, "{how}");
        for (at, model) in models.iter().enumerate() {
            let ended = match (how, at) {
                ("error", 1) => "rejected",
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
            json!({"reserved_micros": 250 * TURNS_LIMIT, "requests": []}),
            "{how}"
        );
        let reader = whizbang_core::store::Reader::open(&world.root(), world.fs.clone());
        let reserved = ledger.reservations(&reader.unwrap()).unwrap();
        let [reservation] = reserved.as_slice() else {
            panic!("{how}: {} reservations", reserved.len());
        };
        assert_eq!((reservation.run(), reservation.micros()), (&run, 4000));
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
                ("error", 1) => assert_eq!(answer["fault"]["kind"], "rejected"),
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
        assert_eq!(record["record"], "run/4");
        assert_eq!(record.as_object().unwrap().len(), 13, "{how}");
    }
}

// --- the use case source-fidelity (plan 2A, task S2) -------------------------------------------

/// `check` under source-fidelity: the other as the candidate, the edit of the
/// photograph, which is the source.
fn fidelity(world: &World) -> Call {
    let job = whizbang_squint::job::Job::shorthand("source-fidelity", true, 0, None);
    judged_call(&job, &[world.other.id(), world.photo.id()])
}

/// `check` under the job's use case, of these images, in the job's order.
pub fn judged_call(job: &whizbang_squint::job::Job, images: &[ArtifactId]) -> Call {
    let mut call = Call::new("check")
        .argument("use_case", Value::text(&job.use_case))
        .argument("job", Value::text(&job.to_json()));
    for image in images {
        call = call.input("images", *image);
    }
    call
}

/// A submission of these items, as the judge's one call.
fn submission(items: Json) -> Json {
    json!({"tool_call": {"id": "s1", "name": "submit_verdict", "arguments": {
        "schema_version": 1, "items": items, "findings": [],
        "opinion": {"verdict": "pass", "reason": "Recorded only."}}}})
}

fn item(id: &str, disposition: &str, confidence: &str) -> Json {
    let location = (disposition == "violation").then(|| {
        json!({
        "kind": "bbox", "frame": "raw:candidate", "bbox": [0.1, 0.1, 0.4, 0.4],
        "polygon": null, "yaw": null, "pitch": null, "fov": null})
    });
    json!({"id": id, "disposition": disposition, "confidence": confidence,
           "reason": format!("{id} was looked at."), "allowed_by": null,
           "evidence": [], "location": location})
}

/// The verdict the run's record holds, read by a reader that opens the store anew.
fn verdict_of(world: &World, finished: &Finished) -> Option<(Decision, String)> {
    let left = left(&world.root(), world.fs.clone(), finished.run());
    let record = left.record?;
    record
        .verdict()
        .map(|(decision, why)| (decision, why.to_owned()))
}

fn finished_with(world: &World, script: &str) -> (Finished, Arc<FakeJudge>) {
    let judge = judge(script, &[]);
    let executor = world.engine(judge.clone(), Secrets::none());
    let finished = executor.submit(&fidelity(world)).unwrap();
    (finished, judge)
}

#[test]
fn source_fidelity_reaches_a_verdict_from_a_scripted_judge() {
    let world = World::new();
    let cases = [
        (("held", "confident"), ("held", "confident"), Decision::Pass),
        (
            ("held", "confident"),
            ("violation", "confident"),
            Decision::Fail,
        ),
        (
            ("held", "confident"),
            ("violation", "tentative"),
            Decision::Unsure,
        ),
        (
            ("cant_tell", "confident"),
            ("held", "confident"),
            Decision::Unsure,
        ),
        (
            ("held", "tentative"),
            ("held", "confident"),
            Decision::Unsure,
        ),
    ];
    for ((first, how_first), (second, how_second), decided) in cases {
        let items = json!([
            item("chk:ob:SF1@candidate", first, how_first),
            item("chk:ob:SF2@candidate", second, how_second),
        ]);
        let script = turns(&[(json!([submission(items)]), "tool_use")]);
        let (finished, judge) = finished_with(&world, &script);
        let Outcome::Completed { outputs } = finished.outcome() else {
            panic!("{:?}", finished.outcome());
        };
        assert!(outputs.is_empty());
        let (decision, why) =
            verdict_of(&world, &finished).unwrap_or_else(|| panic!("{first} {second}: no verdict"));
        assert_eq!(decision, decided, "{first} {second}");
        // The reasons are the deciding items', each by its id.
        assert!(why.contains("chk:ob:"), "{why}");
        // What the judge was told: squint's rules and checklist, the candidate first,
        // and the tool it answers with beside view.
        let asked = &judge.received()[0];
        assert!(
            asked
                .system()
                .contains("SF1 (must hold; judges candidate; consults source)")
        );
        let opening: Vec<String> = asked.messages()[0]
            .parts()
            .iter()
            .filter_map(|part| match part {
                Part::Text(text) => Some(text.clone()),
                _ => None,
            })
            .collect();
        assert!(opening[0].starts_with("Image `candidate` (role Candidate"));
        assert!(opening[1].starts_with("Image `source` (role Source"));
        assert!(opening[2].contains("- chk:ob:SF2@candidate: "));
        let tools: Vec<&str> = asked.tools().iter().map(|tool| tool.name()).collect();
        assert_eq!(tools, ["view", "submit_verdict"]);
    }
    // One image is not a pair: the call is invalid.
    let judge = judge(&turns(&[]), &[]);
    let executor = world.engine(judge, Secrets::none());
    let one = Call::new("check")
        .input("images", world.photo.id())
        .argument("use_case", Value::text("source-fidelity"));
    let finished = executor.submit(&one).unwrap();
    assert!(
        matches!(finished.outcome(), Outcome::Invalid { .. }),
        "{:?}",
        finished.outcome()
    );
}

#[test]
fn a_submission_that_is_not_a_verdict_is_reported_as_invalid() {
    let world = World::new();
    // An item left out, then an item of no checklist: each is answered with why, the
    // first asked for again once, and the second ends the run.
    let missing = submission(json!([item("chk:ob:SF1@candidate", "held", "confident")]));
    let unknown = submission(json!([
        item("chk:ob:SF1@candidate", "held", "confident"),
        item("chk:ob:SF2@candidate", "held", "confident"),
        item("chk:ob:SF9@candidate", "held", "confident"),
    ]));
    let script = turns(&[
        (json!([missing]), "tool_use"),
        (json!([unknown]), "tool_use"),
    ]);
    let (finished, judge) = finished_with(&world, &script);
    match finished.outcome() {
        Outcome::Error { kind, message } => {
            assert_eq!(kind, "invalid_submission");
            assert!(
                message.contains("an item is not on the checklist"),
                "{message}"
            );
        }
        other => panic!("{other:?}"),
    }
    assert!(verdict_of(&world, &finished).is_none());
    // The repair: the rejection, as the call's error, and the words that ask again.
    let received = judge.received();
    assert_eq!(received.len(), 2);
    let repair = received[1].messages().last().unwrap().parts().to_vec();
    match &repair[0] {
        Part::ToolResult(result) => {
            assert!(result.is_error());
            assert_eq!(
                result.text(),
                "Submission rejected: chk:ob:SF2@candidate has no disposition"
            );
        }
        other => panic!("{other:?}"),
    }
    assert!(
        matches!(&repair[1], Part::Text(text) if text.starts_with("Repair: call submit_verdict once"))
    );
    // A disposition no item takes, and a violation with no location, are not verdicts.
    let bad = |items: Json, said: &str| {
        let script = turns(&[
            (json!([submission(items.clone())]), "tool_use"),
            (json!([submission(items)]), "tool_use"),
        ]);
        let (finished, _) = finished_with(&world, &script);
        let Outcome::Error { message, .. } = finished.outcome() else {
            panic!("{:?}", finished.outcome());
        };
        assert!(message.contains(said), "{message}");
    };
    bad(
        json!([
            item("chk:ob:SF1@candidate", "observed", "confident"),
            item("chk:ob:SF2@candidate", "held", "confident")
        ]),
        "chk:ob:SF1@candidate: the disposition is not one of",
    );
    let mut nowhere = item("chk:ob:SF1@candidate", "violation", "confident");
    nowhere["location"] = Json::Null;
    bad(
        json!([nowhere, item("chk:ob:SF2@candidate", "held", "confident")]),
        "a violation has a location",
    );
}

// --- the four other use cases, and the job (plan 2C, task A1) ---------------------------------

use whizbang_squint::job::Job;
use whizbang_squint::use_case::{self, Checklist};

/// The checklist of a job, as the run will make it.
fn checklist_of(job: &Job) -> Checklist {
    Checklist::build(&use_case::resolve(job).unwrap(), job)
}

/// A script whose one turn submits these answers.
fn submits(answers: Json) -> String {
    let call = json!({"tool_call": {"id": "s1", "name": "submit_verdict", "arguments": answers}});
    turns(&[(json!([call]), "tool_use")])
}

/// Runs a call against this script: what the run ended as, what it left, and the judge.
fn judged(world: &World, call: &Call, script: &str) -> (Finished, Left, Arc<FakeJudge>) {
    let judge = judge(script, &[]);
    let executor = world.engine(judge.clone(), Secrets::none());
    let finished = executor.submit(call).unwrap();
    drop(executor);
    let left = left(&world.root(), world.fs.clone(), finished.run());
    (finished, left, judge)
}

/// The images a job of this use case is given: the other as the candidate, then the
/// photograph as its one source or reference.
fn verdicts_of(world: &World, name: &str, brief: Option<&str>) {
    let job = super::use_case::fitting(name, brief);
    let images: Vec<ArtifactId> = [world.other.id(), world.photo.id()]
        .into_iter()
        .take(job.images.len())
        .collect();
    let checklist = checklist_of(&job);
    let last = checklist.ids().last().unwrap().to_owned();
    for (disposition, confidence, decided) in [
        ("held", "confident", Decision::Pass),
        ("violation", "confident", Decision::Fail),
        ("cant_tell", "confident", Decision::Unsure),
    ] {
        let answers = super::use_case::answers(&checklist, &last, disposition, confidence);
        let (finished, left, judge) = judged(world, &judged_call(&job, &images), &submits(answers));
        assert!(
            matches!(finished.outcome(), Outcome::Completed { .. }),
            "{name}"
        );
        let (decision, why) = left.record().verdict().unwrap();
        assert_eq!(decision, decided, "{name} {disposition}");
        assert!(why.contains(&last) || decided == Decision::Pass, "{why}");
        let asked = &judge.received()[0];
        assert!(asked.system().contains("\nRules:\n1. "), "{name}");
        let tools: Vec<&str> = asked.tools().iter().map(|tool| tool.name()).collect();
        assert_eq!(tools, ["view", "submit_verdict"]);
        let kept: Json =
            serde_json::from_slice(left.diagnostic(whizbang_squint::check::JUDGED)).unwrap();
        assert_eq!(kept["use_case"], name);
        assert_eq!(
            kept["checklist"].as_array().unwrap().len(),
            checklist.items.len()
        );
    }
}

#[test]
fn reference_consistency_reaches_a_verdict_from_a_scripted_judge() {
    verdicts_of(&World::new(), "reference-consistency", None);
}

#[test]
fn reality_checks_reaches_a_verdict_from_a_scripted_judge() {
    verdicts_of(&World::new(), "reality-checks", None);
}

#[test]
fn brief_compliance_reaches_a_verdict_from_a_scripted_judge() {
    verdicts_of(
        &World::new(),
        "brief-compliance",
        Some("A red square on grey. No text."),
    );
}

#[test]
fn product_fidelity_reaches_a_verdict_from_a_scripted_judge() {
    verdicts_of(&World::new(), "product-fidelity", None);
}

#[test]
fn a_call_missing_a_required_role_is_refused_at_prepare_and_sends_nothing() {
    let world = World::new();
    let (photo, other) = (world.photo.id(), world.other.id());
    let job = |name: &str, source: bool, references: usize, brief: Option<&str>| {
        Job::shorthand(name, source, references, brief)
    };
    let cases = [
        // A role it requires is missing.
        (
            job("reference-consistency", false, 0, None),
            vec![other],
            "requires an image with role \"reference\"",
        ),
        (
            job("source-fidelity", false, 0, None),
            vec![other],
            "requires an image with role \"source\"",
        ),
        // A role it does not take is named.
        (
            job("reality-checks", true, 0, None),
            vec![other, photo],
            "takes no image with role \"source\"",
        ),
        (
            job("product-fidelity", true, 1, None),
            vec![other, photo, photo],
            "takes no image with role \"source\"",
        ),
        // brief-compliance without a brief.
        (
            job("brief-compliance", false, 0, None),
            vec![other],
            "brief-compliance is given a brief",
        ),
        // A job that names more images than the call gives.
        (
            job("reference-consistency", false, 2, None),
            vec![other, photo],
            "the job names 3 images",
        ),
    ];
    for (job, images, said) in cases {
        let (finished, _, judge) = judged(&world, &judged_call(&job, &images), &turns(&[]));
        match finished.outcome() {
            Outcome::Invalid { why } => assert!(why.contains(said), "{why}"),
            other => panic!("{other:?}"),
        }
        assert_eq!(judge.requests(), 0, "{said}");
    }
    // A call under a use case with no job at all.
    let bare = Call::new("check")
        .input("images", other)
        .argument("use_case", Value::text("reality-checks"));
    let (finished, _, judge) = judged(&world, &bare, &turns(&[]));
    assert!(matches!(finished.outcome(), Outcome::Invalid { .. }));
    assert_eq!(judge.requests(), 0);
}

#[test]
fn references_are_sent_in_the_order_given_with_their_labels() {
    let world = World::new();
    let job = Job::shorthand("product-fidelity", false, 2, None);
    let images = [world.other.id(), world.photo.id(), world.other.id()];
    let checklist = checklist_of(&job);
    let answers = super::use_case::answers(&checklist, "", "held", "confident");
    let (finished, left, judge) = judged(&world, &judged_call(&job, &images), &submits(answers));
    assert!(matches!(finished.outcome(), Outcome::Completed { .. }));
    let opening = judge.received()[0].messages()[0].parts().to_vec();
    let texts: Vec<&str> = opening
        .iter()
        .filter_map(|part| match part {
            Part::Text(text) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    let expected = [
        ("candidate", "Candidate", "", images[0]),
        ("reference-1", "Reference", ", reference 1", images[1]),
        ("reference-2", "Reference", ", reference 2", images[2]),
    ];
    for (at, (id, role, label, image)) in expected.into_iter().enumerate() {
        assert_eq!(
            texts[at],
            format!(
                "Image `{id}` (role {role}{label}, frame raw:{id}, 96x64, Flat); for the tool \
                 view it is named {image}."
            )
        );
        // Each picture follows its line.
        assert!(matches!(opening[2 * at + 1], Part::Image(_)));
    }
    assert!(
        texts[3].contains("- chk:ref:reference-1: ")
            && texts[3].contains("- chk:ref:reference-2: ")
    );
    let kept: Json =
        serde_json::from_slice(left.diagnostic(whizbang_squint::check::JUDGED)).unwrap();
    let labels: Vec<&Json> = kept["images"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| &i["label"])
        .collect();
    assert_eq!(
        labels,
        [&Json::Null, &json!("reference 1"), &json!("reference 2")]
    );
}

#[test]
fn a_violation_located_outside_the_candidate_is_clamped_and_kept() {
    let world = World::new();
    let job = Job::shorthand("source-fidelity", true, 0, None);
    let checklist = checklist_of(&job);
    let mut answers =
        super::use_case::answers(&checklist, "chk:ob:SF1@candidate", "violation", "confident");
    // Wholly past the right and the bottom of the frame, and a finding partly past it.
    answers["items"][0]["location"]["bbox"] = json!([1.2, 1.1, 1.5, 1.4]);
    answers["findings"] = json!([{"obligation_ids": ["SF4"], "disposition": "observed",
        "confidence": "confident", "reason": "A smudge.", "allowed_by": null, "evidence": [],
        "location": {"kind": "bbox", "frame": "raw:candidate", "bbox": [-0.2, 0.5, 0.3, 0.9],
                     "polygon": null, "yaw": null, "pitch": null, "fov": null}}]);
    let call = judged_call(&job, &[world.other.id(), world.photo.id()]);
    let (finished, left, judge) = judged(&world, &call, &submits(answers));
    assert!(
        matches!(finished.outcome(), Outcome::Completed { .. }),
        "{:?}",
        finished.outcome()
    );
    assert_eq!(judge.requests(), 1, "accepted at once, not repaired");
    assert_eq!(left.record().verdict().unwrap().0, Decision::Fail);
    let kept: Json =
        serde_json::from_slice(left.diagnostic(whizbang_squint::check::JUDGED)).unwrap();
    let item = &kept["answers"][0];
    assert_eq!(item["location"]["bbox"], json!([1.0, 1.0, 1.0, 1.0]));
    assert_eq!(item["clamped"], true);
    assert_eq!(item["row"], "fail");
    let finding = &kept["answers"][2];
    assert_eq!(finding["location"]["bbox"], json!([0.0, 0.5, 0.3, 0.9]));
    assert_eq!(finding["clamped"], true);
    assert_eq!(kept["answers"][1]["clamped"], false);
}

#[test]
fn a_retryable_fault_is_retried_after_its_wait_and_a_final_one_ends_the_loop() {
    let world = World::new();
    let job = Job::shorthand("source-fidelity", true, 0, None);
    let call = judged_call(&job, &[world.other.id(), world.photo.id()]);
    let answers = super::use_case::answers(&checklist_of(&job), "", "held", "confident");
    let fault = |kind: &str, wait: Option<u64>| json!({"fault": {"kind": kind, "sentence": "not now", "retry_after_ms": wait}});
    let with_faults = |faults: Vec<Json>| {
        let mut script: Json = serde_json::from_str(&submits(answers.clone())).unwrap();
        let entries = script["entries"].as_array_mut().unwrap();
        for (at, fault) in faults.into_iter().enumerate() {
            entries.insert(at, fault);
        }
        script.to_string()
    };
    // Two the backend could not answer now, then the turn: sent three times, after the
    // waits the backend said.
    let script = with_faults(vec![
        fault("unavailable", Some(5)),
        fault("timed_out", Some(7)),
    ]);
    let (finished, left, judge) = judged(&world, &call, &script);
    assert!(
        matches!(finished.outcome(), Outcome::Completed { .. }),
        "{:?}",
        finished.outcome()
    );
    assert_eq!(judge.requests(), 3);
    let kept: Json =
        serde_json::from_slice(left.diagnostic(whizbang_squint::check::JUDGED)).unwrap();
    assert_eq!(
        kept["retries"],
        json!([{"kind": "unavailable", "wait_ms": 5}, {"kind": "timed_out", "wait_ms": 7}])
    );
    // A fault that is final ends the loop at once.
    let (finished, _, judge) = judged(&world, &call, &with_faults(vec![fault("rejected", None)]));
    assert!(matches!(finished.outcome(), Outcome::Error { kind, .. } if kind == "judge_fault"));
    assert_eq!(judge.requests(), 1);
    // And so does one more than the run may retry.
    let four = (0..4).map(|_| fault("unavailable", Some(1))).collect();
    let (finished, _, judge) = judged(&world, &call, &with_faults(four));
    assert!(matches!(finished.outcome(), Outcome::Error { kind, .. } if kind == "judge_fault"));
    assert_eq!(
        judge.requests(),
        u64::from(whizbang_squint::check::RETRIES_LIMIT) + 1
    );
}

#[test]
fn a_repair_of_an_invalid_submission_is_recorded_in_the_run() {
    let world = World::new();
    let job = Job::shorthand("source-fidelity", true, 0, None);
    let call = judged_call(&job, &[world.other.id(), world.photo.id()]);
    let whole = super::use_case::answers(&checklist_of(&job), "", "held", "confident");
    let mut short = whole.clone();
    short["items"].as_array_mut().unwrap().pop();
    let submit = |answers: Json| json!([{"tool_call": {"id": "s1", "name": "submit_verdict", "arguments": answers}}]);
    let script = turns(&[(submit(short), "tool_use"), (submit(whole), "tool_use")]);
    let (finished, left, judge) = judged(&world, &call, &script);
    assert!(
        matches!(finished.outcome(), Outcome::Completed { .. }),
        "{:?}",
        finished.outcome()
    );
    assert_eq!(judge.requests(), 2);
    assert_eq!(left.record().verdict().unwrap().0, Decision::Pass);
    let kept: Json =
        serde_json::from_slice(left.diagnostic(whizbang_squint::check::JUDGED)).unwrap();
    assert_eq!(
        kept["repairs"],
        json!(["chk:ob:SF2@candidate has no disposition"])
    );
}

// --- the details first, and squint's arm C (plan m2c, task A2) ---------------------------------

/// A plane of squint's vectors, as an image (`fixtures/align/`).
fn plane(name: &str) -> Image {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/align");
    let file = std::fs::read(dir.join(format!("{name}.png"))).unwrap();
    image_in(
        file,
        whizbang_core::identity::Projection::Flat,
        whizbang_core::identity::Topology::Bounded,
    )
}

/// A world that also holds a pair that registers, the source and its restyled, turned
/// candidate, and a scene unrelated to them.
fn paired() -> (World, Image, Image, Image) {
    let world = World::new();
    let (source, candidate, unrelated) = (plane("scene"), plane("registered"), plane("unrelated"));
    world.hold(&[&source, &candidate, &unrelated]);
    (world, source, candidate, unrelated)
}

/// `check` of a pair under source-fidelity, with these arguments beside.
fn of_pair(source: &Image, candidate: &Image, arguments: &[(&str, &str)]) -> Call {
    let job = whizbang_squint::job::Job::shorthand("source-fidelity", true, 0, None);
    let mut call = judged_call(&job, &[candidate.id(), source.id()]);
    for (name, value) in arguments {
        call = call.argument(name, Value::text(value));
    }
    call
}

/// A judge that submits a verdict at once, or after the calls given.
fn submitting(before: &[Json]) -> String {
    let items = json!([
        item("chk:ob:SF1@candidate", "held", "confident"),
        item("chk:ob:SF2@candidate", "held", "confident"),
    ]);
    let mut script: Vec<(Json, &str)> = before
        .iter()
        .map(|call| (json!([call]), "tool_use"))
        .collect();
    script.push((json!([submission(items)]), "tool_use"));
    turns(&script)
}

fn texts(parts: &[Part]) -> Vec<String> {
    parts
        .iter()
        .filter_map(|part| match part {
            Part::Text(text) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn details_first_on_a_reference_pair_aligns_the_candidate_onto_its_reference() {
    // The review of task A2 (#189): details first under a use case of references,
    // product-fidelity, measures the candidate against its reference as it does
    // against a source.
    let (world, reference, candidate, _) = paired();
    let items = json!([
        item("chk:ob:PF1@candidate", "held", "confident"),
        item("chk:ob:PF2@candidate", "held", "confident"),
        item("chk:ob:PF3@candidate", "held", "confident"),
        item("chk:ref:reference-1", "held", "confident"),
    ]);
    let judge = judge(&turns(&[(json!([submission(items)]), "tool_use")]), &[]);
    let executor = world.engine(judge.clone(), Secrets::none());
    let job = whizbang_squint::job::Job::shorthand("product-fidelity", false, 1, None);
    let call = judged_call(&job, &[candidate.id(), reference.id()])
        .argument("details", Value::text("first"));
    let finished = executor.submit(&call).unwrap();
    drop(executor);
    assert_eq!(
        finished.outcome().name(),
        "completed",
        "{:?}",
        finished.outcome()
    );
    let left = left(&world.root(), world.fs.clone(), finished.run());
    assert_eq!(steps(left.record()), [("align", Some(Status::Completed))]);
    let said = texts(judge.received()[0].messages()[0].parts());
    assert!(said[1].starts_with("Image `reference-1`"), "{said:?}");
    assert_eq!(said[2], whizbang_squint::details::DETAILS_INTRO);
    assert!(
        said[3].starts_with("align (unverified), candidate onto reference-1: "),
        "{}",
        said[3]
    );
    assert!(
        said[4]
            .starts_with("Image `aligned:candidate:t1` (candidate warped into reference-1's grid"),
        "{}",
        said[4]
    );
}

#[test]
fn details_first_puts_the_aligned_pair_in_the_opening_message_before_the_first_turn() {
    let (world, source, candidate, _) = paired();
    let judge = judge(&submitting(&[]), &[]);
    let executor = world.engine(judge.clone(), Secrets::none());
    let call = of_pair(&source, &candidate, &[("details", "first")]);
    let finished = executor.submit(&call).unwrap();
    drop(executor);
    assert!(
        matches!(finished.outcome(), Outcome::Completed { .. }),
        "{:?}",
        finished.outcome()
    );
    let left = left(&world.root(), world.fs.clone(), finished.run());
    // `align` ran through the executor, as a step of the run, before the judge was asked.
    let record = left.record();
    assert_eq!(steps(record), [("align", Some(Status::Completed))]);
    assert_eq!(judge.requests(), 1);
    // What the opening holds, in order: the two images, then the details, then the
    // checklist. The details are the tool, its mark, squint's sentence and every
    // measurement, then the aligned frame, named, and its picture.
    let want = whizbang_squint::align::align(
        &candidate,
        &source,
        &whizbang_squint::align::Options::default(),
        &limits(),
    )
    .unwrap();
    let frame = want.frame.clone().unwrap().image;
    let opening = judge.received()[0].messages()[0].parts().to_vec();
    let said = texts(&opening);
    assert!(said[0].starts_with("Image `candidate`"), "{said:?}");
    assert!(said[1].starts_with("Image `source`"), "{said:?}");
    assert_eq!(said[2], whizbang_squint::details::DETAILS_INTRO);
    assert!(
        said[3].starts_with(
            "align (unverified), candidate onto source: registered candidate registered onto \
             source: shift "
        ),
        "{}",
        said[3]
    );
    assert!(said[3].contains("The aligned frame aligned:candidate:t1 shares source's grid"));
    for name in [
        "translation_px",
        "rotation_deg",
        "scale",
        "perspective",
        "ncc",
        "inlier_fraction",
        "overlap",
        "texture",
    ] {
        assert!(said[3].contains(&format!("{name} ")), "{name}: {}", said[3]);
    }
    assert!(said[3].contains("\nMeasurements: translation_px "));
    assert!(said[4].starts_with(
        "Image `aligned:candidate:t1` (candidate warped into source's grid, 128x96, Flat)"
    ));
    assert!(said[4].ends_with(&format!("named {}.", frame.id())));
    assert!(said[5].starts_with("Checklist."));
    let pictures: Vec<_> = opening
        .iter()
        .filter_map(|part| match part {
            Part::Image(picture) => Some(picture.digest()),
            _ => None,
        })
        .collect();
    assert_eq!(
        pictures,
        [
            candidate.identity().pixels,
            source.identity().pixels,
            frame.identity().pixels
        ]
    );
    // The aligned frame is the run's evidence, and the judge may name it.
    assert_eq!(left.evidence(1).id(), frame.id());
    // A pair that does not register is said too, with no picture: not registered is itself
    // evidence that the geometry changed.
    let (world, source, _, unrelated) = paired();
    let judge = self::judge(&submitting(&[]), &[]);
    let executor = world.engine(judge.clone(), Secrets::none());
    let finished = executor
        .submit(&of_pair(&source, &unrelated, &[("details", "first")]))
        .unwrap();
    assert!(matches!(finished.outcome(), Outcome::Completed { .. }));
    let said = texts(judge.received()[0].messages()[0].parts());
    assert!(
        said[3].starts_with(
            "align (unverified), candidate onto source: not_registered candidate does not \
             register onto source: agreement "
        ),
        "{}",
        said[3]
    );
    assert!(said[4].starts_with("Checklist."));
}

#[test]
fn details_none_opens_as_the_baseline_recorded_arm_c() {
    let (world, source, candidate, _) = paired();
    let judge = judge(&submitting(&[]), &[]);
    let executor = world.engine(judge.clone(), Secrets::none());
    let arm_c = [
        ("tools", "view,align"),
        ("details", "none"),
        ("projection", "visual_only"),
    ];
    let finished = executor
        .submit(&of_pair(&source, &candidate, &arm_c))
        .unwrap();
    drop(executor);
    assert!(matches!(finished.outcome(), Outcome::Completed { .. }));
    let left = left(&world.root(), world.fs.clone(), finished.run());
    assert!(steps(left.record()).is_empty());
    let asked = &judge.received()[0];
    // The tools in squint's prompt order, `align` as squint's judge was told of it.
    let tools: Vec<&str> = asked.tools().iter().map(|tool| tool.name()).collect();
    assert_eq!(tools, ["view", "align", "submit_verdict"]);
    let align = &asked.tools()[1];
    assert!(
        align
            .description()
            .starts_with("Registers one image onto another")
    );
    assert_eq!(align.schema()["required"], json!(["image", "against"]));
    assert_eq!(align.schema()["additionalProperties"], json!(false));
    // Squint's Tools section, between the obligations and how to answer.
    let tools_section = "\n\nTools. Use them to look closer before deciding; every call is \
        logged. Refer to images and frames by their ids.\n- view (Cheap, about 1s): Shows a \
        region closer: ";
    assert!(asked.system().contains(tools_section), "{}", asked.system());
    assert!(asked.system().contains(
        "\n- align (Moderate, about 3s): Registers one image onto another of the same scene \
         and viewpoint"
    ));
    assert!(asked.system().contains(
        "a reshaped pool or wall inside the frame registers as well as an unchanged one.\n\n\
         Answer with the submit_verdict tool"
    ));
    // The opening: the two images, the checklist and the instruction, and nothing measured.
    let said = texts(asked.messages()[0].parts());
    assert_eq!(said.len(), 4, "{said:?}");
    assert!(said[0].starts_with("Image `candidate`"));
    assert!(said[1].starts_with("Image `source`"));
    assert!(said[2].starts_with("Checklist."));
    assert_eq!(said[3], whizbang_squint::use_case::INSTRUCTION);
    // With no evidence tool, there is no Tools section, as squint's prompt had none.
    let judge = self::judge(&submitting(&[]), &[]);
    let executor = world.engine(judge.clone(), Secrets::none());
    let none = [("tools", ""), ("details", "none")];
    executor
        .submit(&of_pair(&source, &candidate, &none))
        .unwrap();
    let system = judge.received()[0].system().to_owned();
    assert!(!system.contains("\nTools."), "{system}");
    assert!(
        system.contains("`allowed_by`.\n\n\nAnswer with"),
        "{system}"
    );
}

/// A call of `align` of the candidate onto the source, by squint's names.
fn aligning(id: &str) -> Json {
    json!({"tool_call": {"id": id, "name": "align",
        "arguments": {"image": "candidate", "against": "raw:source"}}})
}

#[test]
fn under_visual_only_a_call_of_align_is_answered_with_its_pictures_and_squints_summary_alone() {
    let (world, source, candidate, unrelated) = paired();
    let answered = |candidate: &Image, projection: &str| {
        let judge = judge(&submitting(&[aligning("a1")]), &[]);
        let executor = world.engine(judge.clone(), Secrets::none());
        let asked = [
            ("tools", "view,align"),
            ("details", "none"),
            ("projection", projection),
        ];
        let finished = executor
            .submit(&of_pair(&source, candidate, &asked))
            .unwrap();
        drop(executor);
        assert!(matches!(finished.outcome(), Outcome::Completed { .. }));
        results(&judge, 1).remove(0)
    };
    let want = whizbang_squint::align::align(
        &candidate,
        &source,
        &whizbang_squint::align::Options::default(),
        &limits(),
    )
    .unwrap();
    let frame = want.frame.clone().unwrap().image;
    let params = want.transform.clone().unwrap().params.to_vec();
    // Squint's presentation: the call, ok, the fixed sentence, nothing measured, no region,
    // and the frame with its place on the source; keys in order, on one line.
    let result = answered(&candidate, "visual_only");
    assert!(!result.is_error());
    let expected = json!({
        "call_id": "call:0", "status": "ok",
        "summary": "Result computed; its measurements are withheld.",
        "measurements": [], "regions": [],
        "images": [{"frame": "aligned:candidate:t1", "parent": "raw:source",
                    "mapping": {"homography": params}}],
    })
    .to_string();
    assert_eq!(result.text(), expected);
    assert!(
        result
            .text()
            .starts_with("{\"call_id\":\"call:0\",\"images\":[{\"frame\":")
    );
    let shown: Vec<_> = result.images().iter().map(|p| p.digest()).collect();
    assert_eq!(shown, [frame.identity().pixels]);
    // A pair that does not register: not applicable, the other fixed sentence, no picture.
    let result = answered(&unrelated, "visual_only");
    assert_eq!(
        result.text(),
        json!({"call_id": "call:0", "status": "not_applicable",
               "summary": "Result not available.", "measurements": [], "regions": [],
               "images": []})
        .to_string()
    );
    assert!(result.images().is_empty());
    // Under full, the same call is answered with everything: squint's sentence and the
    // measurements whole, and the region of a pair that does not register.
    let full: Json = serde_json::from_str(answered(&candidate, "full").text()).unwrap();
    assert_eq!(full["status"], "ok");
    assert_eq!(
        full["summary"],
        whizbang_squint::align::summary(&want, "candidate", "source")
    );
    assert_eq!(full["measurements"].as_array().unwrap().len(), 8);
    assert_eq!(full["measurements"][4]["name"], "ncc");
    assert_eq!(full["measurements"][4]["value"], json!(want.quality.ncc));
    let full: Json = serde_json::from_str(answered(&unrelated, "full").text()).unwrap();
    assert_eq!(full["status"], "not_applicable");
    assert_eq!(full["regions"][0]["label"], "not registered onto source");
}

#[test]
fn a_use_case_with_one_image_adds_no_details() {
    let world = World::new();
    let script = turns(&[(json!([{"text": "pass\nClean."}]), "end_turn")]);
    let judge = judge(&script, &[]);
    let executor = world.engine(judge.clone(), Secrets::none());
    let call = world
        .check()
        .argument("details", Value::text("first"))
        .argument("tools", Value::text("view,align,resize"));
    let finished = executor.submit(&call).unwrap();
    drop(executor);
    assert!(matches!(finished.outcome(), Outcome::Completed { .. }));
    let left = left(&world.root(), world.fs.clone(), finished.run());
    assert!(steps(left.record()).is_empty());
    // The opening is the image, named, and its picture: nothing was measured.
    let opening = judge.received()[0].messages()[0].parts().to_vec();
    assert_eq!(opening.len(), 2);
    assert!(matches!(&opening[0], Part::Text(text) if text.starts_with("image ")));
    // A detail that is no tool measured first is refused before anything runs.
    let judge = self::judge(&script, &[]);
    let executor = world.engine(judge.clone(), Secrets::none());
    let wrong = world
        .check()
        .argument("details", Value::text("first"))
        .argument("detail", Value::text("view"));
    let finished = executor.submit(&wrong).unwrap();
    assert!(
        matches!(finished.outcome(), Outcome::Invalid { .. }),
        "{:?}",
        finished.outcome()
    );
    assert_eq!(judge.requests(), 0);
}

// --- the tools that compare a pair, measured first and called (plan m2c, task X1) -------------

#[test]
fn an_experimental_tool_is_run_only_when_the_run_names_it() {
    let (world, source, candidate, _) = paired();
    // Not named: the details first run align alone, though change_map is on offer.
    let first = judge(&submitting(&[]), &[]);
    let executor = world.engine(first, Secrets::none());
    let call = of_pair(&source, &candidate, &[("details", "first")]);
    let finished = executor.submit(&call).unwrap();
    drop(executor);
    let alone = left(&world.root(), world.fs.clone(), finished.run());
    assert_eq!(steps(alone.record()), [("align", Some(Status::Completed))]);
    // Named alone: align runs first, then each comparison on the source and the frame align
    // returned, and the judge's opening holds what each measured, labelled with its mark.
    let judge = judge(&submitting(&[]), &[]);
    let executor = world.engine(judge.clone(), Secrets::none());
    let named = [
        ("details", "first"),
        ("detail", "change_map,edge_similarity"),
    ];
    let finished = executor
        .submit(&of_pair(&source, &candidate, &named))
        .unwrap();
    drop(executor);
    assert!(
        matches!(finished.outcome(), Outcome::Completed { .. }),
        "{:?}",
        finished.outcome()
    );
    let left = left(&world.root(), world.fs.clone(), finished.run());
    assert_eq!(
        steps(left.record()),
        [
            ("align", Some(Status::Completed)),
            ("change_map", Some(Status::Completed)),
            ("edge_similarity", Some(Status::Completed)),
        ]
    );
    let aligned = whizbang_squint::align::align(
        &candidate,
        &source,
        &whizbang_squint::align::Options::default(),
        &limits(),
    )
    .unwrap()
    .frame
    .unwrap()
    .image;
    let (changes, map, _) =
        whizbang_squint::compare::change_map(&source, &aligned, &limits()).unwrap();
    let edges = whizbang_squint::compare::edge_similarity(&source, &aligned, &limits()).unwrap();
    let opening = judge.received()[0].messages()[0].parts().to_vec();
    let said = texts(&opening);
    let change = said
        .iter()
        .find(|text| {
            text.starts_with("change_map (unverified), aligned:candidate:t1 against source: ")
        })
        .unwrap_or_else(|| panic!("{said:?}"));
    assert!(
        change.contains(&format!(
            "\nMeasurements: changed_fraction {:.4} ratio.\n",
            changes.fraction
        )),
        "{change}"
    );
    assert!(change.contains("blind to a change of colour"), "{change}");
    let map_line = said
        .iter()
        .find(|text| text.starts_with("Image `map:candidate:source`"))
        .unwrap();
    assert!(
        map_line.ends_with(&format!("named {}.", map.id())),
        "{map_line}"
    );
    let edge = said
        .iter()
        .find(|text| {
            text.starts_with("edge_similarity (unverified), aligned:candidate:t1 against source: ")
        })
        .unwrap();
    assert!(
        edge.contains(&format!(
            "\nMeasurements: edge_similarity {:.4} ratio; edge_energy {:.4} intensity per pixel.\n",
            edges.similarity, edges.energy
        )),
        "{edge}"
    );
    let pictures: Vec<_> = opening
        .iter()
        .filter_map(|part| match part {
            Part::Image(picture) => Some(picture.digest()),
            _ => None,
        })
        .collect();
    assert_eq!(
        pictures,
        [
            candidate.identity().pixels,
            source.identity().pixels,
            aligned.identity().pixels,
            map.identity().pixels,
        ]
    );
}

#[test]
fn a_comparison_named_by_a_run_is_offered_to_the_judge_and_answered_with_its_measurements() {
    let (world, source, candidate, unrelated) = paired();
    // The judge compares the source with the unrelated scene, by their names.
    let compares = one_call(
        "edge_similarity",
        json!({"frame": "source", "against": unrelated.id().to_string()}),
    );
    let general = judge(&compares, &[]);
    let executor = world.engine(general.clone(), Secrets::none());
    let call = Call::new("check")
        .input("images", source.id())
        .input("images", unrelated.id())
        .argument("tools", Value::text("view,edge_similarity"));
    let _ = executor.submit(&call).unwrap();
    drop(executor);
    let offered: Vec<String> = general.received()[0]
        .tools()
        .iter()
        .map(|tool| tool.name().to_owned())
        .collect();
    assert_eq!(offered, ["view", "edge_similarity"]);
    let schema = general.received()[0].tools()[1].schema().clone();
    assert_eq!(schema["required"], json!(["frame", "against"]));
    // In general use, an image is named by its identity alone: "source" is no name.
    let first = results(&general, 1);
    assert!(first[0].is_error(), "{}", first[0].text());
    assert!(
        first[0].text().contains("unknown image id"),
        "{}",
        first[0].text()
    );
    // Under a use case, by its role; answered as squint's harness answered it.
    let compares = json!({"tool_call": {"id": "t1", "name": "edge_similarity",
        "arguments": {"frame": "source", "against": "candidate"}}});
    let script = submitting(&[compares]);
    let judge = judge(&script, &[]);
    let executor = world.engine(judge.clone(), Secrets::none());
    let call = of_pair(
        &source,
        &candidate,
        &[("details", "none"), ("tools", "view,align,edge_similarity")],
    );
    let finished = executor.submit(&call).unwrap();
    drop(executor);
    assert!(
        matches!(finished.outcome(), Outcome::Completed { .. }),
        "{:?}",
        finished.outcome()
    );
    let answered = results(&judge, 1);
    let said: Json = serde_json::from_str(answered[0].text()).unwrap();
    let edges = whizbang_squint::compare::edge_similarity(&source, &candidate, &limits()).unwrap();
    assert_eq!(said["status"], "ok");
    assert_eq!(said["measurements"][0]["name"], "edge_similarity");
    assert_eq!(said["measurements"][0]["value"], json!(edges.similarity));
    assert_eq!(said["measurements"][0]["frame"], "source");
    assert!(
        said["summary"]
            .as_str()
            .unwrap()
            .starts_with("Edge similarity of source and candidate: "),
        "{said}"
    );
}

// --- a tool file offered to a judge (plan m2c, task X1, item 4) ----------------------------

/// A tool file as the binary hands `check` one: `probe`, which reads `source` and
/// `candidate` and takes a `phrase`; each run noted, and its output a fixed reading.
#[derive(Default)]
struct Probe {
    ran: std::sync::Mutex<Vec<Probed>>,
    /// The digest it says it ran, when not [`PROBE_DIGEST`]: a file edited mid-run.
    ran_as: Option<&'static str>,
}

type Probed = (Vec<(String, ArtifactId)>, Json);

const PROBE_DIGEST: &str =
    "sha256:abababababababababababababababababababababababababababababababab";

impl whizbang_squint::files::FileTools for Probe {
    fn listed(&self) -> Vec<whizbang_squint::files::FileTool> {
        vec![whizbang_squint::files::FileTool {
            name: "probe".to_owned(),
            purpose: "Probes how much of a named subject the candidate keeps.".to_owned(),
            bindings: vec!["source".to_owned(), "candidate".to_owned()],
            arguments: vec![whizbang_squint::files::FileArgument {
                name: "phrase".to_owned(),
                number: false,
                purpose: "The subject, in words.".to_owned(),
                required: true,
            }],
        }]
    }

    fn revision(&self, _: &str) -> Option<String> {
        Some(PROBE_DIGEST.to_owned())
    }

    fn run(
        &self,
        _: &str,
        images: &[(String, ArtifactId)],
        arguments: &serde_json::Map<String, Json>,
        _: &whizbang_core::contract::Context,
    ) -> Result<whizbang_squint::files::FileRan, String> {
        let mut ran = self.ran.lock().unwrap();
        ran.push((images.to_vec(), Json::Object(arguments.clone())));
        Ok(whizbang_squint::files::FileRan {
            digest: self.ran_as.unwrap_or(PROBE_DIGEST).to_owned(),
            output: json!({"readings": [{"subject": "door", "values": {"kept": 0.75}}]}),
        })
    }
}

fn probing(world: &mut World) -> Arc<Probe> {
    let probe = Arc::new(Probe::default());
    world.toolbox = whizbang_squint::check::Toolbox::with_files(probe.clone());
    probe
}

#[test]
fn a_named_tool_file_is_offered_to_the_judge_with_its_schema_and_called() {
    let (mut world, source, candidate, _) = paired();
    let probe = probing(&mut world);
    let asked = json!({"tool_call": {"id": "p1", "name": "probe", "arguments":
        {"source": "source", "candidate": "candidate", "phrase": "door"}}});
    let judge = judge(&submitting(&[asked]), &[]);
    let executor = world.engine(judge.clone(), Secrets::none());
    let call = of_pair(&source, &candidate, &[("tools", "view,probe")]);
    let finished = executor.submit(&call).unwrap();
    drop(executor);
    assert!(
        matches!(finished.outcome(), Outcome::Completed { .. }),
        "{:?}",
        finished.outcome()
    );
    // Offered with the schema its declaration gives: an image for each binding, and its
    // argument, each required.
    let offered = judge.received()[0].tools().to_vec();
    let tool = offered
        .iter()
        .find(|tool| tool.name() == "probe")
        .expect("offered");
    assert_eq!(
        tool.schema()["required"],
        json!(["source", "candidate", "phrase"])
    );
    assert_eq!(
        tool.schema()["properties"]["phrase"]["type"],
        json!("string")
    );
    // Called once, with the run's images by the names the judge gave them, and answered
    // with its output and the digest it ran.
    let ran = probe.ran.lock().unwrap().clone();
    assert_eq!(
        ran,
        [(
            vec![
                ("source".to_owned(), source.id()),
                ("candidate".to_owned(), candidate.id())
            ],
            json!({"phrase": "door"})
        )]
    );
    let said = results(&judge, 1);
    assert!(!said[0].is_error());
    assert_eq!(
        said[0].text(),
        format!(
            "call:0: probe (experimental; a tool file, {PROBE_DIGEST}) measured: \
             {{\"readings\":[{{\"subject\":\"door\",\"values\":{{\"kept\":0.75}}}}]}}"
        )
    );
    let left = left(&world.root(), world.fs.clone(), finished.run());
    let noted = left
        .record()
        .asked()
        .settings
        .iter()
        .any(|setting| setting.name() == "tool_file:probe");
    assert!(noted);
}

#[test]
fn a_tool_file_not_named_is_never_offered() {
    let (mut world, source, candidate, _) = paired();
    let probe = probing(&mut world);
    // On offer, and not named: the judge is not told of it, and a call of it is refused
    // before anything runs.
    let asked = json!({"tool_call": {"id": "p1", "name": "probe", "arguments":
        {"source": "source", "candidate": "candidate", "phrase": "door"}}});
    let judge = judge(&submitting(&[asked]), &[]);
    let executor = world.engine(judge.clone(), Secrets::none());
    let finished = executor.submit(&of_pair(&source, &candidate, &[])).unwrap();
    drop(executor);
    assert!(
        matches!(finished.outcome(), Outcome::Completed { .. }),
        "{:?}",
        finished.outcome()
    );
    assert!(
        judge.received()[0]
            .tools()
            .iter()
            .all(|tool| tool.name() != "probe")
    );
    let said = results(&judge, 1);
    assert!(said[0].is_error());
    assert!(
        said[0].text().starts_with("refused: "),
        "{}",
        said[0].text()
    );
    assert!(probe.ran.lock().unwrap().is_empty());
    // A run that names a tool file nothing offers is refused as invalid.
    let judge = judge_none();
    let executor = world.engine(judge, Secrets::none());
    let named = of_pair(&source, &candidate, &[("tools", "view,absent")]);
    let finished = executor.submit(&named).unwrap();
    assert!(
        matches!(finished.outcome(), Outcome::Invalid { .. }),
        "{:?}",
        finished.outcome()
    );
}

fn judge_none() -> Arc<FakeJudge> {
    judge(&submitting(&[]), &[])
}

#[test]
fn a_run_whose_tool_file_was_edited_after_its_key_was_made_keeps_no_key() {
    // The key holds each tool file's digest when the run begins; a call that ran the
    // file as another digest leaves the run's verdict unkept (the review of task X1).
    let decided = |ran_as: Option<&'static str>| {
        let (mut world, source, candidate, _) = paired();
        world.cached = true;
        let probe = Arc::new(Probe {
            ran_as,
            ..Probe::default()
        });
        world.toolbox = whizbang_squint::check::Toolbox::with_files(probe);
        let asked = json!({"tool_call": {"id": "p1", "name": "probe", "arguments":
            {"source": "source", "candidate": "candidate", "phrase": "door"}}});
        let script = submitting(&[asked]);
        let turns = whizbang_core::capability::written::read_script(script.as_bytes()).unwrap();
        let turns = turns
            .into_iter()
            .map(|turn| turn.map(|turn| turn.reported_by(Some("fake-judge"))))
            .collect();
        let judge = Arc::new(FakeJudge::scripted(turns));
        let executor = world.engine(judge, Secrets::none());
        let call = of_pair(&source, &candidate, &[("tools", "view,probe")]);
        let finished = executor.submit(&call).unwrap();
        drop(executor);
        let left = left(&world.root(), world.fs.clone(), finished.run());
        let settings = left.record().asked().settings.clone();
        let said = settings
            .iter()
            .find(|s| s.name() == "decision_cache")
            .map(|s| format!("{:?}", s.value()));
        said.unwrap_or_default()
    };
    assert!(decided(None).contains("\"kept\""), "{}", decided(None));
    let edited = decided(Some(
        "sha256:cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd",
    ));
    assert!(
        edited.contains("not kept: a tool file it ran was edited"),
        "{edited}"
    );
}

// --- the second look (plan m3, task U1, item 4) -----------------------------------------------

/// A reality-checks job on the photograph, its call with the second look as given.
fn reality(world: &World, second_look: &str) -> Call {
    let job = super::use_case::fitting("reality-checks", None);
    judged_call(&job, &[world.photo.id()]).argument("second_look", Value::text(second_look))
}

/// The four items of reality-checks: `RE2` as given, with this location; the rest held,
/// confidently.
fn reality_items(disposition: &str, confidence: &str, location: Json) -> Json {
    let mut items = Vec::new();
    for rule in ["RE1", "RE2", "RE3", "RE4"] {
        let id = format!("chk:ob:{rule}@candidate");
        if rule == "RE2" {
            let mut said = item(&id, disposition, confidence);
            said["location"] = location.clone();
            items.push(said);
        } else {
            items.push(item(&id, "held", "confident"));
        }
    }
    json!(items)
}

const RE2: &str = "chk:ob:RE2@candidate";

fn a_box(bbox: [f64; 4]) -> Json {
    json!({"kind": "bbox", "frame": "raw:candidate", "bbox": bbox, "polygon": null,
           "yaw": null, "pitch": null, "fov": null})
}

/// A script that submits the first items, then, asked again, the second.
fn twice(first: Json, second: Json) -> String {
    turns(&[
        (json!([submission(first)]), "tool_use"),
        (json!([submission(second)]), "tool_use"),
    ])
}

fn pictures_of(parts: &[Part]) -> Vec<whizbang_core::identity::PixelDigest> {
    parts
        .iter()
        .filter_map(|part| match part {
            Part::Image(picture) => Some(picture.digest()),
            _ => None,
        })
        .collect()
}

fn second_look_kept(left: &Left) -> Json {
    let kept: Json =
        serde_json::from_slice(left.diagnostic(whizbang_squint::check::JUDGED)).unwrap();
    kept["second_look"].clone()
}

#[test]
fn a_tentative_mandatory_item_gets_one_second_look_with_the_cited_crops() {
    let world = World::new();
    let cited = [0.2, 0.3, 0.5, 0.6];
    let first = reality_items("held", "tentative", a_box(cited));
    let settled = json!([item(RE2, "held", "confident")]);
    let (finished, left, judge) = judged(&world, &reality(&world, "on"), &twice(first, settled));
    assert_eq!(
        finished.outcome().name(),
        "completed",
        "{:?}",
        finished.outcome()
    );
    // One more turn: the submission is answered, the cited region shown at full
    // resolution with a margin of a twentieth, and the item asked about alone.
    assert_eq!(judge.requests(), 2);
    let asked = judge.received()[1]
        .messages()
        .last()
        .unwrap()
        .parts()
        .to_vec();
    let Part::ToolResult(received) = &asked[0] else {
        panic!("{:?}", asked[0]);
    };
    assert_eq!(received.call(), "s1");
    assert!(received.text().contains(&format!("leaves {RE2} tentative")));
    let margin = [
        cited[0] - 0.05,
        cited[1] - 0.05,
        cited[2] + 0.05,
        cited[3] + 0.05,
    ];
    let crop = crop_of(&world.photo, margin);
    assert_eq!(pictures_of(&asked), [crop.identity().pixels]);
    let said = texts(&asked);
    assert!(
        said[0].starts_with(&format!("Second look at {RE2}.")),
        "{said:?}"
    );
    assert!(said[1].starts_with("Crop `look:1` of candidate: left 0.15, top 0.25"));
    assert!(
        said[2].starts_with(&format!("Settle {RE2} alone.")),
        "{said:?}"
    );
    // The crop is a call of the run, and its evidence.
    assert_eq!(steps(left.record()), [("view", Some(Status::Completed))]);
    assert_eq!(left.evidence(1).id(), crop.id());
    assert_eq!(left.record().verdict().unwrap().0, Decision::Pass);
    let look = second_look_kept(&left);
    assert_eq!(look["items"], json!([RE2]));
    assert_eq!(look["crops"], 1);
    assert_eq!(look["outcome"], "settled");
    assert_eq!(look["verdict_before"], "unsure");
    assert_eq!(look["verdict_after"], "pass");
    assert_eq!(look["before"][0]["confidence"], "tentative");
    assert_eq!(look["after"][0]["confidence"], "confident");
    // An item that cites no region is shown the candidate in its four tiles.
    let world = World::new();
    let first = reality_items("held", "tentative", Json::Null);
    let settled = json!([item(RE2, "held", "confident")]);
    let (_, left, judge) = judged(&world, &reality(&world, "on"), &twice(first, settled));
    let asked = judge.received()[1]
        .messages()
        .last()
        .unwrap()
        .parts()
        .to_vec();
    let tiles: Vec<_> = whizbang_squint::details::TILE_BOXES
        .iter()
        .map(|(_, bbox)| crop_of(&world.photo, *bbox).identity().pixels)
        .collect();
    assert_eq!(pictures_of(&asked), tiles);
    assert_eq!(second_look_kept(&left)["crops"], 4);
    // Once a run: a second submission that is still tentative is the verdict.
    let world = World::new();
    let first = reality_items("held", "tentative", Json::Null);
    let again = json!([item(RE2, "held", "tentative")]);
    let (_, left, judge) = judged(&world, &reality(&world, "on"), &twice(first, again));
    assert_eq!(judge.requests(), 2);
    assert_eq!(left.record().verdict().unwrap().0, Decision::Unsure);
    assert_eq!(second_look_kept(&left)["outcome"], "settled");
}

#[test]
fn a_verdict_after_a_second_look_is_still_worked_out_from_the_dispositions() {
    // A violation seen, confidently: fail, whatever the submission's opinion says.
    let world = World::new();
    let first = reality_items("held", "tentative", Json::Null);
    let seen = json!([{
        "id": RE2, "disposition": "violation", "confidence": "confident",
        "reason": "The shadow falls the wrong way.", "allowed_by": null, "evidence": [],
        "location": a_box([0.1, 0.1, 0.3, 0.3])}]);
    let (_, left, _) = judged(&world, &reality(&world, "on"), &twice(first, seen));
    let (decision, why) = left.record().verdict().unwrap();
    assert_eq!(decision, Decision::Fail);
    assert!(
        why.starts_with(&format!("{RE2}: violation (confident)")),
        "{why}"
    );
    // A second submission that answers an item it was not asked about settles nothing:
    // the first verdict stands, and the look is kept with why.
    let world = World::new();
    let first = reality_items("held", "tentative", Json::Null);
    let other = json!([item("chk:ob:RE1@candidate", "violation", "confident")]);
    let (finished, left, _) = judged(&world, &reality(&world, "on"), &twice(first, other));
    assert_eq!(finished.outcome().name(), "completed");
    assert_eq!(left.record().verdict().unwrap().0, Decision::Unsure);
    let look = second_look_kept(&left);
    assert_eq!(look["outcome"], "kept");
    assert_eq!(look["verdict_after"], "unsure");
    // So does a turn that ends without a submission.
    let world = World::new();
    let first = reality_items("held", "tentative", Json::Null);
    let script = turns(&[
        (json!([submission(first)]), "tool_use"),
        (json!([{"text": "pass\nAll fine."}]), "end_turn"),
    ]);
    let (_, left, _) = judged(&world, &reality(&world, "on"), &script);
    assert_eq!(left.record().verdict().unwrap().0, Decision::Unsure);
    assert_eq!(second_look_kept(&left)["outcome"], "kept");
}

#[test]
fn second_look_off_runs_as_before() {
    for second_look in ["off", "default"] {
        let world = World::new();
        let first = reality_items("held", "tentative", Json::Null);
        let settled = json!([item(RE2, "held", "confident")]);
        let call = if second_look == "off" {
            reality(&world, "off")
        } else {
            let job = super::use_case::fitting("reality-checks", None);
            judged_call(&job, &[world.photo.id()])
        };
        let (_, left, judge) = judged(&world, &call, &twice(first, settled));
        assert_eq!(judge.requests(), 1, "{second_look}");
        assert_eq!(left.record().verdict().unwrap().0, Decision::Unsure);
        assert!(steps(left.record()).is_empty());
        assert_eq!(second_look_kept(&left), Json::Null);
    }
    // And a verdict that is not unsure is given no second look, the argument on.
    let world = World::new();
    let first = reality_items("held", "confident", Json::Null);
    let (_, left, judge) = judged(&world, &reality(&world, "on"), &twice(first, json!([])));
    assert_eq!(judge.requests(), 1);
    assert_eq!(left.record().verdict().unwrap().0, Decision::Pass);
}

#[test]
fn the_second_look_is_inside_the_runs_turns_and_spend() {
    // What a call reserves is the same with the second look as without: it covers every
    // turn of the loop, and the second look is one of them.
    let world = World::new();
    let script = twice(
        reality_items("held", "tentative", Json::Null),
        json!([item(RE2, "held", "confident")]),
    );
    for second_look in ["off", "on"] {
        let executor = world.engine_priced(judge(&script, &[]), Secrets::none(), |a| a, 250);
        let prepared = executor.prepare(&reality(&world, second_look)).unwrap();
        assert_eq!(prepared.estimate().micros(), 250 * TURNS_LIMIT as u64);
    }
    // The second look is recorded as a turn of its own, beside the first.
    let (_, left, _) = judged(&world, &reality(&world, "on"), &script);
    let turns_recorded = (left.record().judge().iter())
        .filter(|judged| matches!(judged, Judged::Turn { .. }))
        .count();
    assert_eq!(turns_recorded, 2);
    // A submission in the run's last turn is given none: the verdict stands as unsure.
    let world = World::new();
    let view = |n: usize| {
        json!([{"tool_call": {"id": format!("v{n}"), "name": "view",
                              "arguments": looking_at(world.photo.id(), None)}}])
    };
    let mut script: Vec<(Json, &str)> = (0..TURNS_LIMIT - 1)
        .map(|n| (view(n), "tool_use"))
        .collect();
    script.push((
        json!([submission(reality_items("held", "tentative", Json::Null))]),
        "tool_use",
    ));
    script.push((
        json!([submission(json!([item(RE2, "held", "confident")]))]),
        "tool_use",
    ));
    let (finished, left, judge) = judged(&world, &reality(&world, "on"), &turns(&script));
    assert_eq!(
        finished.outcome().name(),
        "completed",
        "{:?}",
        finished.outcome()
    );
    assert_eq!(judge.requests(), TURNS_LIMIT as u64);
    assert_eq!(left.record().verdict().unwrap().0, Decision::Unsure);
    assert_eq!(second_look_kept(&left), Json::Null);
}
