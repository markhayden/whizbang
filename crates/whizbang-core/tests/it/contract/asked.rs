//! What a run asks of its capabilities (ADR 0009): an action is handed the backend its
//! call resolved and no other; each request it sends is in its run's record, with the
//! model that answered and how long it took; each exchange with a judge is kept whole
//! as a diagnostic and summarised in the record; an opaque part's body is never
//! stored; and a verdict is written with a run that completed, and with no other.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Value as Json, json};
use whizbang_core::capability::written::EXCHANGE;
use whizbang_core::capability::{
    Controls, Excluded, Fault, FaultKind, FillRequest, Judge, JudgeRequest, Message, Opaque, Part,
    Picture, REQUESTS_LIMIT, Role, Roster, StopReason, Support, ToolCall, Turn, Usage,
};
use whizbang_core::contract::record::{
    Decision, Judged, MODELS_LIMIT, RequestEnded, RunRecord, Status, Stop, Timed,
};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Backends, Call, Capabilities, Capability, Clock, Context,
    EXCHANGE_PREFIX, Engine, Executor, Failure, Kind, PreparedCall, Produced, RECORD_MAX_BYTES,
    RecordError, Registry, Requirement, RunId, Secrets, ValueType, Verdict,
};
use whizbang_core::contract::{MESSAGE_LIMIT, Value};
use whizbang_core::identity::{CanonicalPixels, Digest};
use whizbang_core::store::{Kind as Stored, Reader};
use whizbang_testkit::capabilities::{FakeFiller, FakeJudge, Paint, roster};
use whizbang_testkit::schema::validate;

use super::commit::World;
use super::record::schema;
use super::redact::made_up_key;
use crate::capability::written::BODY;

/// A clock that says one millisecond more each time it is asked.
struct Ticking(AtomicU64);

impl Clock for Ticking {
    fn now_ms(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst)
    }
}

type Body = Box<dyn Fn(&PreparedCall, &Context) -> Result<Produced, Failure> + Send + Sync>;

struct Scripted(ActionDescriptor, Body);

impl Action for Scripted {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        (self.1)(call, context)
    }
}

/// An action of this name with these needs, whose run is whatever the test says. It
/// takes an optional text, `judge`, which a need may name as the argument that chooses
/// its backend.
fn action(
    name: &str,
    needs: Vec<Requirement>,
    body: impl Fn(&PreparedCall, &Context) -> Result<Produced, Failure> + Send + Sync + 'static,
) -> Arc<dyn Action> {
    let mut draft = ActionDescriptor::draft(name, "1.0.0", Kind::Primitive, "A test action.")
        .argument(Argument::optional("judge", "A judge.", ValueType::Text));
    for need in needs {
        draft = draft.needs(need);
    }
    Arc::new(Scripted(draft.build().unwrap(), Box::new(body)))
}

fn judging() -> Vec<Requirement> {
    vec![Requirement::new(Capability::Judge).chosen_by("judge")]
}

fn engine(
    world: &World,
    backends: impl Backends + 'static,
    held: Capabilities,
    actions: Vec<Arc<dyn Action>>,
) -> Engine {
    let mut registry = Registry::new();
    for action in actions {
        registry.register(action).unwrap();
    }
    Engine::over(
        registry,
        Arc::new(backends),
        world.store(),
        Digest::of_bytes(b""),
    )
    .with_clock(Arc::new(Ticking(AtomicU64::new(1000))))
    .with_capabilities(held)
}

/// The run's record, read from the store and held to the schema, and everything else
/// the run holds there, by name.
fn stored(world: &World, run: &RunId) -> (RunRecord, BTreeMap<String, Vec<u8>>) {
    let reader = Reader::open(&world.root(), world.fs.clone()).unwrap();
    let generation = reader.snapshot().unwrap();
    let entries = generation.run(run.as_str()).unwrap().entries();
    let mut names = BTreeMap::new();
    for (name, entry) in entries {
        assert_eq!(entry.kind, Stored::Diagnostic, "{name}");
        names.insert(name.clone(), reader.read(entry).unwrap());
    }
    let record = RunRecord::from_bytes(&names["record"]).unwrap();
    let document: Json = serde_json::from_slice(&names["record"]).unwrap();
    validate(&schema(), &document).unwrap();
    assert!((names["record"].len() as u64) < RECORD_MAX_BYTES);
    (record, names)
}

fn said(text: &str) -> Turn {
    let parts = vec![Part::Text(text.to_owned())];
    Turn::new(parts, StopReason::EndTurn, None, None).unwrap()
}

fn asking(text: &str, tokens: u32) -> JudgeRequest {
    let message = Message::new(Role::User, vec![Part::Text(text.to_owned())]).unwrap();
    let controls = Controls::new(tokens).unwrap();
    JudgeRequest::new("You judge.", vec![message], Vec::new(), controls).unwrap()
}

fn fill_request() -> FillRequest {
    let rgba = (0..24u8).flat_map(|at| [at, at, at, 255]).collect();
    let picture = Picture::new(CanonicalPixels::from_rgba(6, 4, rgba).unwrap()).unwrap();
    let mut plane = vec![0; 24];
    plane[..6].fill(255);
    FillRequest::new(picture, Support::from_plane(6, 4, plane).unwrap(), None).unwrap()
}

type Noted<T> = Arc<Mutex<Vec<T>>>;

// --- which backend an action is handed --------------------------------------------------------

#[test]
fn an_action_is_handed_the_backend_its_call_resolved_and_no_other() {
    // The executor holds two judges and a filler. A call that names the second judge
    // is handed the second, one that names none is handed the default, and neither is
    // handed the filler, which its call does not need. An action that needs nothing
    // is handed nothing.
    let world = World::new();
    let one = Arc::new(FakeJudge::scripted(vec![Ok(said("one"))]).as_model("judge-one"));
    let two = Arc::new(FakeJudge::scripted(vec![Ok(said("two"))]).as_model("judge-two"));
    let filler = Arc::new(FakeFiller::new(Paint::Inverse));
    let offered = roster(None, Some(filler.as_ref()), Some(one.as_ref()))
        .offer(Capability::Judge, "second", two.declaration().clone())
        .unwrap();
    let held = Capabilities::none()
        .with_judge(one.clone())
        .with_judge(two.clone())
        .with_fill(filler.clone());
    let seen: Noted<(Option<String>, bool, bool)> = Noted::default();
    let body = |seen: &Noted<(Option<String>, bool, bool)>| {
        let seen = seen.clone();
        move |_: &PreparedCall, context: &Context| {
            let handles = context.capabilities();
            let judge = handles.judge();
            let model = judge.map(|judge| judge.declaration().model().model().to_owned());
            let noted = (model, handles.fill().is_some(), handles.segment().is_some());
            seen.lock().unwrap().push(noted);
            if let Some(judge) = judge {
                judge.send(&asking("well?", 64)).unwrap();
            }
            Ok(Produced::nothing())
        }
    };
    let actions = vec![
        action("asks", judging(), body(&seen)),
        action("plain", Vec::new(), body(&seen)),
    ];
    let executor = engine(&world, offered, held, actions);

    let second = Call::new("asks").argument("judge", Value::text("second"));
    executor.submit(&second).unwrap();
    assert_eq!((one.requests(), two.requests()), (0, 1));
    executor.submit(&Call::new("asks")).unwrap();
    assert_eq!((one.requests(), two.requests()), (1, 1));
    executor.submit(&Call::new("plain")).unwrap();
    assert_eq!(
        (one.requests(), two.requests(), filler.requests()),
        (1, 1, 0)
    );
    let judged = |model: &str| (Some(model.to_owned()), false, false);
    assert_eq!(
        *seen.lock().unwrap(),
        [
            judged("judge-two"),
            judged("judge-one"),
            (None, false, false)
        ]
    );
}

#[test]
fn known_a_call_that_resolved_a_model_the_executor_does_not_hold_is_handed_no_handle() {
    // The limit, pinned as known: the roster and the backends an executor holds are
    // given apart, and nothing holds one to the other. A call that resolved a model no
    // held backend declares is admitted, and its action finds no handle.
    let world = World::new();
    let offered = Arc::new(FakeJudge::scripted(Vec::new()).as_model("judge-elsewhere"));
    let held = Arc::new(FakeJudge::scripted(Vec::new()));
    let seen: Noted<bool> = Noted::default();
    let asks = {
        let seen = seen.clone();
        action("asks", judging(), move |_, context| {
            let handed = context.capabilities().judge().is_some();
            seen.lock().unwrap().push(handed);
            Err(Failure::error("no_judge", "no judge was handed over"))
        })
    };
    let executor = engine(
        &world,
        roster(None, None, Some(offered.as_ref())),
        Capabilities::none().with_judge(held.clone()),
        vec![asks],
    );
    let finished = executor.submit(&Call::new("asks")).unwrap();
    assert_eq!(finished.outcome().name(), "error");
    assert_eq!(*seen.lock().unwrap(), [false]);
    assert_eq!(held.requests(), 0);
}

// --- the requests in the record ---------------------------------------------------------------

#[test]
fn each_request_a_run_sends_is_in_its_record_with_its_model_how_long_it_took_and_how_it_ended() {
    let world = World::new();
    let filler = Arc::new(FakeFiller::new(Paint::Inverse));
    let unavailable = Fault::new(FaultKind::Unavailable, "overloaded");
    let judge = Arc::new(FakeJudge::scripted(vec![Ok(said("yes")), Err(unavailable)]));
    let asks = action(
        "asks",
        vec![
            Requirement::new(Capability::Fill),
            Requirement::new(Capability::Judge),
        ],
        |_, context| {
            let handles = context.capabilities();
            handles.fill().unwrap().fill(&fill_request()).unwrap();
            handles.judge().unwrap().send(&asking("one", 64)).unwrap();
            context.executor().submit(&Call::new("inner")).unwrap();
            handles
                .judge()
                .unwrap()
                .send(&asking("two", 64))
                .unwrap_err();
            Ok(Produced::nothing())
        },
    );
    let inner = action("inner", Vec::new(), |_, _| Ok(Produced::nothing()));
    let executor = engine(
        &world,
        roster(None, Some(filler.as_ref()), Some(judge.as_ref())),
        Capabilities::none()
            .with_fill(filler.clone())
            .with_judge(judge.clone()),
        vec![asks, inner],
    );
    let finished = executor.submit(&Call::new("asks")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    let (record, _) = stored(&world, finished.run());

    // Every request, in the order they were sent, with the model its backend declares.
    let models: Vec<_> = record
        .models()
        .iter()
        .map(|m| {
            (
                m.capability,
                m.backend.as_str(),
                m.model.as_str(),
                m.version.as_str(),
                m.ended,
            )
        })
        .collect();
    assert_eq!(
        models,
        [
            (
                Capability::Fill,
                "fake",
                "fake-filler",
                "1",
                RequestEnded::Answered
            ),
            (
                Capability::Judge,
                "fake",
                "fake-judge",
                "1",
                RequestEnded::Answered
            ),
            (
                Capability::Judge,
                "fake",
                "fake-judge",
                "1",
                RequestEnded::Unavailable
            ),
        ]
    );
    // Which device answered and whether a cache did are not known: no response says.
    assert!(
        record
            .models()
            .iter()
            .all(|m| m.device.is_none() && m.cached.is_none())
    );
    // A duration for each request, then one for the call, by the executor's clock:
    // each began after the run did, and the clock ticked at least once before it ended.
    let durations = &record.time().durations;
    let of: Vec<_> = durations.iter().map(|d| (d.of, d.index)).collect();
    assert_eq!(
        of,
        [
            (Timed::Request, 0),
            (Timed::Request, 1),
            (Timed::Request, 2),
            (Timed::Call, 0)
        ]
    );
    for duration in durations {
        assert!(duration.start_ms > record.time().start_ms, "{duration:?}");
        assert!(duration.ms >= 1, "{duration:?}");
        assert!(
            duration.start_ms + duration.ms <= record.time().end_ms,
            "{duration:?}"
        );
    }
    assert!(durations[0].start_ms < durations[1].start_ms);
    // The run that was called sent nothing, and its record lists nothing.
    let child: RunId = RunId::parse(record.steps()[0].run()).unwrap();
    let (child, _) = stored(&world, &child);
    assert!(child.models().is_empty() && child.time().durations.is_empty());
    assert!(child.judge().is_empty());
}

/// A bound far under the one a run has, so that the test publishes few exchanges.
const FEW: usize = 24;

#[test]
fn a_run_sends_only_so_many_requests_and_its_record_lists_each_and_is_read_back() {
    // The judge's script is empty, so each request ends in the fake's fault. The run
    // sends as many as it may; one more is not sent. The bound is the executor's,
    // lowered here through its seam for tests: a run of the real bound publishes
    // 1,024 exchanges, which took six seconds on the test kit's disk.
    let world = World::new();
    let judge = Arc::new(FakeJudge::scripted(Vec::new()));
    let asks = action("asks", judging(), |_, context| {
        let judge = context.capabilities().judge().unwrap();
        for _ in 0..FEW {
            let fault = judge.send(&asking("again", 64)).unwrap_err();
            assert_eq!(fault.kind(), FaultKind::Malformed);
        }
        for _ in 0..2 {
            let fault = judge.send(&asking("once more", 64)).unwrap_err();
            assert_eq!(fault.kind(), FaultKind::TooLarge);
            let said = "the run has sent 24 requests, which is as many as one run may";
            assert_eq!(fault.sentence(), said);
        }
        Ok(Produced::nothing())
    });
    let executor = engine(
        &world,
        roster(None, None, Some(judge.as_ref())),
        Capabilities::none().with_judge(judge.clone()),
        vec![asks],
    )
    .with_requests_limit(FEW);
    let finished = executor.submit(&Call::new("asks")).unwrap();
    assert_eq!(judge.requests() as usize, FEW);
    let (record, names) = stored(&world, finished.run());
    assert_eq!(record.models().len(), FEW);
    assert_eq!(record.judge().len(), FEW);
    assert_eq!(record.time().durations.len(), FEW);
    assert!(names.contains_key("diagnostics/exchange-0024"));
    assert!(!names.contains_key("diagnostics/exchange-0025"));
}

#[test]
fn the_bound_on_a_runs_requests_is_the_one_its_record_and_the_schema_hold() {
    // The seam only lowers the bound: an executor asked for more keeps the real one,
    // which is what the record's types and the schema refuse one more than.
    assert_eq!((REQUESTS_LIMIT, MODELS_LIMIT), (1024, 1024));
    let limit = schema()["properties"]["models"]["maxItems"].as_u64();
    assert_eq!(limit, Some(REQUESTS_LIMIT as u64));
    let world = World::new();
    let judge = Arc::new(FakeJudge::scripted(Vec::new()));
    let asks = action("asks", judging(), |_, context| {
        let judge = context.capabilities().judge().unwrap();
        for _ in 0..FEW + 1 {
            let fault = judge.send(&asking("again", 64)).unwrap_err();
            assert_eq!(fault.kind(), FaultKind::Malformed);
        }
        Ok(Produced::nothing())
    });
    let executor = engine(
        &world,
        roster(None, None, Some(judge.as_ref())),
        Capabilities::none().with_judge(judge.clone()),
        vec![asks],
    )
    .with_requests_limit(REQUESTS_LIMIT + 1);
    let finished = executor.submit(&Call::new("asks")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    assert_eq!(judge.requests() as usize, FEW + 1);
}

/// A judge whose call panics.
struct Panics(whizbang_core::capability::Declaration);

impl Judge for Panics {
    fn declaration(&self) -> &whizbang_core::capability::Declaration {
        &self.0
    }

    fn send(&self, _: &JudgeRequest, _: &whizbang_core::capability::Bounds) -> Result<Turn, Fault> {
        panic!("the backend fell over")
    }
}

#[test]
fn a_request_whose_backend_panicked_is_in_a_record_that_is_read_back() {
    // The backend's call unwinds through the handle and the action. The request is
    // accounted for as unwound, no exchange was kept, and the run's record, which
    // lists the request and a turn for it with nothing in it, is one the reader takes.
    let world = World::new();
    let fake = FakeJudge::scripted(Vec::new());
    let judge = Arc::new(Panics(fake.declaration().clone()));
    let asks = action("asks", judging(), |_, context| {
        let judge = context.capabilities().judge().unwrap();
        judge
            .send(&asking("well?", 64))
            .map(|_| Produced::nothing())
            .map_err(|fault| Failure::error("judge", fault.sentence()))
    });
    let executor = engine(
        &world,
        roster(None, None, Some(judge.as_ref())),
        Capabilities::none().with_judge(judge.clone()),
        vec![asks],
    );
    let finished = executor.submit(&Call::new("asks")).unwrap();
    assert_eq!(finished.outcome().name(), "error");
    let (record, names) = stored(&world, finished.run());
    assert_eq!(record.outcome().error.as_ref().unwrap().kind, "panic");
    assert_eq!(record.models().len(), 1);
    assert_eq!(record.models()[0].ended, RequestEnded::Unwound);
    assert_eq!(record.time().durations.len(), 1);
    let unanswered = Judged::Turn {
        request: 0,
        exchange: None,
        max_output_tokens: 0,
        messages: 0,
        stop: None,
        tool_calls: 0,
        usage: None,
        provider_id: None,
        excluded: None,
    };
    assert_eq!(record.judge(), [unanswered]);
    assert!(!names.keys().any(|name| name.contains("exchange")));
}

// --- the judge group --------------------------------------------------------------------------

/// A turn of two calls with an opaque part between them.
fn calling() -> Turn {
    let call = |id: &str| {
        let arguments = json!({"left": 0.25});
        Part::ToolCall(ToolCall::new(id, "view", arguments).unwrap())
    };
    let parts = vec![
        Part::Text("I will look.".to_owned()),
        call("call-1"),
        Part::Opaque(Opaque::new(FakeJudge::ADAPTER, BODY).unwrap()),
        call("call-2"),
    ];
    let usage = Usage::new(120, 30).cached(7, 9);
    Turn::new(parts, StopReason::ToolUse, Some(usage), Some("req-1")).unwrap()
}

fn cut_off() -> Turn {
    let parts = vec![Part::Text("The crop shows".to_owned())];
    Turn::new(parts, StopReason::MaxTokens, None, None).unwrap()
}

/// `judges`: sends four requests, the second after the first turn was said back, and
/// says that the cut-off answer was left out. It concludes what the test says.
fn judges(verdict: Verdict) -> Arc<dyn Action> {
    action("judges", judging(), move |_, context| {
        let judge = context.capabilities().judge().unwrap();
        assert!(
            !judge.exclude_last(Excluded::Truncated),
            "nothing was asked yet"
        );
        let first = asking("Look.", 256);
        let turn = judge.send(&first).unwrap();
        let mut messages = first.messages().to_vec();
        messages.push(turn.message().unwrap());
        let controls = Controls::new(256).unwrap();
        let second = JudgeRequest::new("You judge.", messages, Vec::new(), controls).unwrap();
        let cut = judge.send(&second).unwrap();
        assert_eq!(cut.stop(), &StopReason::MaxTokens);
        assert!(judge.exclude_last(Excluded::Truncated));
        judge.send(&asking("again", 1024)).unwrap_err();
        judge.send(&asking("and again", 1024)).unwrap();
        Ok(Produced::nothing().with_verdict(verdict.clone()))
    })
}

fn judged(world: &World, verdict: Verdict, secrets: Secrets) -> (Arc<FakeJudge>, RunId) {
    let rejected = Fault::new(FaultKind::Rejected, "not this");
    let script = vec![
        Ok(calling()),
        Ok(cut_off()),
        Err(rejected),
        Ok(said("pass")),
    ];
    let judge = Arc::new(FakeJudge::scripted(script));
    let executor = engine(
        world,
        roster(None, None, Some(judge.as_ref())),
        Capabilities::none().with_judge(judge.clone()),
        vec![judges(verdict)],
    )
    .with_secrets(secrets);
    let finished = executor.submit(&Call::new("judges")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    (judge, finished.run().clone())
}

#[test]
fn an_exchange_with_a_judge_is_kept_whole_as_a_diagnostic_and_summarised_in_the_record() {
    let world = World::new();
    let verdict = Verdict::new(Decision::Pass, "it looks right");
    let (_, run) = judged(&world, verdict, Secrets::none());
    let (record, names) = stored(&world, &run);

    // One turn for each request to the judge, in order, and then the verdict.
    let turn = |at: usize| match &record.judge()[at] {
        Judged::Turn {
            request,
            exchange,
            max_output_tokens,
            messages,
            stop,
            tool_calls,
            usage,
            provider_id,
            excluded,
        } => {
            assert_eq!(*request, at as u64);
            let name = format!("diagnostics/{EXCHANGE_PREFIX}{:04}", at + 1);
            assert_eq!(exchange.as_deref(), Some(name.as_str()));
            let tokens = usage.map(|u| {
                (
                    u.input_tokens,
                    u.output_tokens,
                    u.cache_read_tokens,
                    u.cache_write_tokens,
                )
            });
            let facts = (*max_output_tokens, *messages, *stop, *tool_calls);
            (facts, tokens, provider_id.clone(), *excluded)
        }
        other => panic!("{other:?}"),
    };
    assert_eq!(
        turn(0),
        (
            (256, 1, Some(Stop::ToolUse), 2),
            Some((120, 30, 7, 9)),
            Some("req-1".to_owned()),
            None
        )
    );
    // The cut-off turn is recorded, and marked as left out.
    assert_eq!(
        turn(1),
        (
            (256, 2, Some(Stop::MaxTokens), 0),
            None,
            None,
            Some(Excluded::Truncated)
        )
    );
    // No turn came back for the third: its fault is with its request.
    assert_eq!(turn(2), ((1024, 1, None, 0), None, None, None));
    assert_eq!(record.models()[2].ended, RequestEnded::Rejected);
    assert_eq!(turn(3).0, (1024, 1, Some(Stop::EndTurn), 0));
    assert_eq!(record.judge().len(), 5);
    assert_eq!(record.verdict(), Some((Decision::Pass, "it looks right")));

    // Each exchange is a diagnostic of the run, listed in its record: what was sent,
    // whole, and what came back.
    let exchange = |at: usize| -> Json {
        let name = format!("diagnostics/{EXCHANGE_PREFIX}{:04}", at + 1);
        assert!(record.diagnostics().iter().any(|d| d.name() == name));
        serde_json::from_slice(&names[&name]).unwrap()
    };
    let first = exchange(0);
    assert_eq!(first["exchange"], EXCHANGE);
    assert_eq!(
        (first["place"].as_u64(), first["system"].as_str()),
        (Some(0), Some("You judge."))
    );
    assert_eq!(first["messages"][0]["parts"][0]["text"], "Look.");
    let parts = &first["answer"]["turn"]["parts"];
    assert_eq!(parts[0]["text"], "I will look.");
    assert_eq!(parts[1]["tool_call"]["id"], "call-1");
    assert_eq!(parts[3]["tool_call"]["arguments"]["left"], json!(0.25));
    // The second was sent the first turn, said back; and holds the cut-off answer.
    let second = exchange(1);
    assert_eq!(second["messages"][1]["role"], "assistant");
    assert_eq!(second["messages"][1]["parts"], *parts);
    assert_eq!(second["answer"]["turn"]["stop"], "max_tokens");
    assert_eq!(
        second["answer"]["turn"]["parts"][0]["text"],
        "The crop shows"
    );
    assert_eq!(exchange(2)["answer"]["fault"]["kind"], "rejected");
}

#[test]
fn an_opaque_parts_body_is_sent_back_whole_and_is_never_stored() {
    // The body is kept whole in memory: the second request holds it byte for byte.
    // What is stored of it is its adapter, its digest and its length.
    let world = World::new();
    let (judge, run) = judged(&world, Verdict::new(Decision::Fail, "no"), Secrets::none());
    let sent = &judge.received()[1];
    let Part::Opaque(part) = &sent.messages()[1].parts()[2] else {
        panic!("the third part of the turn is the opaque one");
    };
    assert_eq!(part.body().as_bytes(), BODY.as_bytes());
    let (_, names) = stored(&world, &run);
    let stored: Json = serde_json::from_slice(&names["diagnostics/exchange-0002"]).unwrap();
    let kept = &stored["messages"][1]["parts"][2]["opaque_withheld"];
    assert_eq!(
        kept["digest"],
        json!(Digest::of_bytes(BODY.as_bytes()).to_string())
    );
    assert_eq!(kept["bytes"], json!(BODY.len()));
    for (path, bytes) in files_under(&world.root()) {
        assert!(!holds(&bytes, b"zz9"), "{path:?} holds a part of the body");
    }
}

fn files_under(root: &std::path::Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else {
                files.push((path.clone(), std::fs::read(&path).unwrap()));
            }
        }
    }
    files
}

fn holds(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[test]
fn a_secret_in_what_a_judge_was_sent_or_answered_is_in_nothing_the_run_stores() {
    // A made-up key in the system prompt, a message, a judge's text, a tool call's
    // arguments, an opaque body, the provider's id, a fault's sentence and the
    // verdict's reason. The action is handed all of it as it came; the store holds
    // none of it, in any file.
    let world = World::new();
    let key = made_up_key("JUDGED");
    let secrets = Secrets::new([key.clone()]).unwrap();
    let call = ToolCall::new("call-1", "view", json!({"image": key, "note": "a/b"})).unwrap();
    let parts = vec![
        Part::Text(format!("I read {key} in the image.")),
        Part::ToolCall(call),
        Part::Opaque(Opaque::new(FakeJudge::ADAPTER, &format!("sig {key}")).unwrap()),
    ];
    let turn = Turn::new(parts, StopReason::ToolUse, None, Some(&key)).unwrap();
    assert_eq!(
        turn.request_id(),
        Some(key.as_str()),
        "cleaning leaves the key whole"
    );
    let fault = Fault::new(FaultKind::Rejected, &format!("refused {key}"));
    let judge = Arc::new(FakeJudge::scripted(vec![Ok(turn), Err(fault)]));
    let handed: Noted<String> = Noted::default();
    let asks = {
        let (key, handed) = (key.clone(), handed.clone());
        action("asks", judging(), move |_, context| {
            let judge = context.capabilities().judge().unwrap();
            let message = vec![Part::Text(format!("the label says {key}"))];
            let message = Message::new(Role::User, message).unwrap();
            let controls = Controls::new(64).unwrap();
            let system = format!("You judge. {key}");
            let first = JudgeRequest::new(&system, vec![message], Vec::new(), controls).unwrap();
            let turn = judge.send(&first).unwrap();
            handed
                .lock()
                .unwrap()
                .extend(turn.texts().map(str::to_owned));
            let said_back = vec![first.messages()[0].clone(), turn.message().unwrap()];
            let second = JudgeRequest::new(&system, said_back, Vec::new(), controls).unwrap();
            let fault = judge.send(&second).unwrap_err();
            handed.lock().unwrap().push(fault.sentence().to_owned());
            let why = format!("{}{key}", "x".repeat(MESSAGE_LIMIT - 5));
            Ok(Produced::nothing().with_verdict(Verdict::new(Decision::Unsure, &why)))
        })
    };
    let executor = engine(
        &world,
        roster(None, None, Some(judge.as_ref())),
        Capabilities::none().with_judge(judge.clone()),
        vec![asks],
    )
    .with_secrets(secrets);
    let finished = executor.submit(&Call::new("asks")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");

    // The action was handed the turn and the fault as they came.
    let handed = handed.lock().unwrap().clone();
    assert!(
        handed[0].contains(&key) && handed[1].contains(&key),
        "{handed:?}"
    );
    // The judge was sent the opaque part back with its key.
    assert_eq!(judge.requests(), 2);
    // Nothing under the store holds the key, as it is or as JSON escapes it; nor the
    // head of it, which a cut through the verdict's reason would have left.
    let files = files_under(&world.root());
    assert!(files.len() > 5);
    for (path, bytes) in &files {
        assert!(!holds(bytes, key.as_bytes()), "{path:?} holds the key");
        assert!(
            !holds(bytes, &key.as_bytes()[..12]),
            "{path:?} holds the head of the key"
        );
    }
    let (record, names) = stored(&world, finished.run());
    let exchange = String::from_utf8(names["diagnostics/exchange-0001"].clone()).unwrap();
    assert!(
        exchange.contains("I read [redacted] in the image."),
        "{exchange}"
    );
    assert!(exchange.contains(r#""image":"[redacted]""#), "{exchange}");
    assert!(exchange.contains("the label says [redacted]"), "{exchange}");
    let Judged::Turn { provider_id, .. } = &record.judge()[0] else {
        panic!("the first entry is a turn");
    };
    assert_eq!(provider_id.as_deref(), Some("[redacted]"));
    // The reason was redacted while it was whole, and then cut.
    let (_, why) = record.verdict().unwrap();
    assert!(
        why.ends_with("x[redacted]") || why.ends_with("..."),
        "{why}"
    );
    assert!(why.chars().count() <= MESSAGE_LIMIT + 3);
}

#[test]
fn an_action_cannot_write_a_diagnostic_under_an_exchanges_name() {
    let world = World::new();
    let writes = action("writes", Vec::new(), |_, context| {
        for name in ["exchange-0001", "exchange-notes"] {
            let refusal = context.diagnostics().write(name, b"mine").unwrap_err();
            assert!(refusal.contains("which the executor writes"), "{refusal}");
        }
        context.diagnostics().write("exchanges", b"mine").unwrap();
        Ok(Produced::nothing())
    });
    let executor = engine(&world, Roster::new(), Capabilities::none(), vec![writes]);
    let finished = executor.submit(&Call::new("writes")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    let (record, _) = stored(&world, finished.run());
    let names: Vec<_> = record.diagnostics().iter().map(|d| d.name()).collect();
    assert_eq!(names, ["diagnostics/exchanges", "journal"]);
}

// --- the verdict ------------------------------------------------------------------------------

#[test]
fn a_verdict_is_written_with_a_run_that_completed_and_with_no_other() {
    let world = World::new();
    let verdict = || Verdict::new(Decision::Fail, "the seam shows");
    let concludes = action("concludes", Vec::new(), move |call, context| {
        let how = call.arguments().first().map(|(_, value)| value.clone());
        match how {
            // An accepted cancellation decides the outcome, whatever is handed back.
            Some(Value::Text(how)) if how == "cancelled" => context.cancellation().cancel(),
            Some(Value::Text(how)) if how == "failed" => {
                return Err(Failure::error("judge", "no answer"));
            }
            _ => {}
        }
        Ok(Produced::nothing().with_verdict(verdict()))
    });
    let executor = engine(&world, Roster::new(), Capabilities::none(), vec![concludes]);
    let run = |how: Option<&str>| {
        let call = Call::new("concludes");
        let call = match how {
            Some(how) => call.argument("judge", Value::text(how)),
            None => call,
        };
        let finished = executor.submit(&call).unwrap();
        stored(&world, finished.run()).0
    };
    let completed = run(None);
    assert_eq!(completed.outcome().status, Status::Completed);
    assert_eq!(
        completed.verdict(),
        Some((Decision::Fail, "the seam shows"))
    );
    let cancelled = run(Some("cancelled"));
    assert_eq!(cancelled.outcome().status, Status::Cancelled);
    assert_eq!(cancelled.verdict(), None);
    assert!(cancelled.judge().is_empty());
    let failed = run(Some("failed"));
    assert_eq!(failed.outcome().status, Status::Error);
    assert_eq!(failed.verdict(), None);
    // What `{:?}` prints of a verdict is its shape.
    assert_eq!(
        format!("{:?}", verdict()),
        "Verdict(Fail, a reason of 14 bytes)"
    );
}

// --- the schema and the types -----------------------------------------------------------------

/// The record of a run that judged, as JSON, with one change made to it.
fn changed(record: &RunRecord, change: &dyn Fn(&mut Json)) -> (Vec<u8>, Json) {
    let mut document: Json = serde_json::from_slice(&record.to_bytes()).unwrap();
    change(&mut document);
    (serde_json::to_vec(&document).unwrap(), document)
}

#[test]
fn what_is_no_model_no_duration_and_no_judge_entry_is_refused_by_the_types_and_by_the_schema() {
    let world = World::new();
    let (_, run) = judged(
        &world,
        Verdict::new(Decision::Pass, "fine"),
        Secrets::none(),
    );
    let (record, _) = stored(&world, &run);
    let both = |what: &str, change: &dyn Fn(&mut Json)| {
        let (bytes, document) = changed(&record, change);
        assert!(
            RunRecord::from_bytes(&bytes).is_err(),
            "the types take {what}"
        );
        assert!(
            validate(&schema(), &document).is_err(),
            "the schema takes {what}"
        );
    };
    // A field that is not known, and each field left out, in each kind of element.
    let elements = [
        (
            "/models/0",
            vec![
                "capability",
                "backend",
                "model",
                "version",
                "device",
                "cached",
                "ended",
            ],
        ),
        ("/time/durations/0", vec!["of", "index", "start_ms", "ms"]),
        (
            "/judge/0",
            vec![
                "judge",
                "request",
                "exchange",
                "max_output_tokens",
                "messages",
                "stop",
                "tool_calls",
                "usage",
                "provider_id",
                "excluded",
            ],
        ),
        (
            "/judge/0/usage",
            vec![
                "input_tokens",
                "output_tokens",
                "cache_read_tokens",
                "cache_write_tokens",
            ],
        ),
        ("/judge/4", vec!["judge", "decision", "why"]),
    ];
    for (path, fields) in &elements {
        both(&format!("an unknown field in {path}"), &|d| {
            d.pointer_mut(path).unwrap()["extra"] = json!(1);
        });
        for field in fields {
            both(&format!("{path} without {field}"), &|d| {
                let element = d.pointer_mut(path).unwrap().as_object_mut().unwrap();
                element.remove(*field).unwrap();
            });
        }
    }
    // A word that is none, a value of the wrong kind, a number under zero.
    let wrong: [(&str, Json); 16] = [
        ("/models/0/capability", json!("paint")),
        ("/models/0/ended", json!("lost")),
        ("/models/0/device", json!("gpu")),
        ("/models/0/cached", json!(false)),
        ("/models/0/model", json!(7)),
        ("/time/durations/0/of", json!("turn")),
        ("/time/durations/0/ms", json!(-1)),
        ("/time/durations/0/index", json!(0.5)),
        ("/judge/0/judge", json!("repair")),
        ("/judge/0/stop", json!("finished")),
        ("/judge/0/excluded", json!("ignored")),
        ("/judge/0/request", json!(-1)),
        ("/judge/0/usage/input_tokens", json!("many")),
        ("/judge/0/provider_id", json!("p".repeat(129))),
        ("/judge/4/decision", json!("maybe")),
        ("/judge/4/why", json!("y".repeat(MESSAGE_LIMIT + 4))),
    ];
    for (path, value) in &wrong {
        both(&format!("{value} at {path}"), &|d| {
            *d.pointer_mut(path).unwrap() = value.clone()
        });
    }
    // A turn with a verdict's fields, and more requests than a run sends.
    both("a turn that is also a verdict", &|d| {
        d["judge"][0]["decision"] = json!("pass")
    });
    both("more models than a run sends", &|d| {
        let model = d["models"][0].clone();
        d["models"] = json!(vec![model; REQUESTS_LIMIT + 1]);
    });
}

#[test]
fn what_the_schema_cannot_say_of_the_requests_and_the_judge_the_types_still_refuse() {
    let world = World::new();
    let (_, run) = judged(
        &world,
        Verdict::new(Decision::Pass, "fine"),
        Secrets::none(),
    );
    let (record, _) = stored(&world, &run);
    let types_alone = |what: &str, change: &dyn Fn(&mut Json)| {
        let (bytes, document) = changed(&record, change);
        validate(&schema(), &document).unwrap_or_else(|why| panic!("{what}: {why}"));
        let error = RunRecord::from_bytes(&bytes).unwrap_err();
        assert!(
            matches!(error, RecordError::Inconsistent(_)),
            "{what}: {error}"
        );
    };
    let remove = |d: &mut Json, list: &str, at: usize| {
        d.pointer_mut(list)
            .unwrap()
            .as_array_mut()
            .unwrap()
            .remove(at)
    };
    types_alone("a request without its duration", &|d| {
        drop(remove(d, "/time/durations", 1))
    });
    types_alone("durations out of their order", &|d| {
        d["time"]["durations"].as_array_mut().unwrap().swap(0, 1);
    });
    types_alone("a duration of a step there is not", &|d| {
        let call = json!({"of": "call", "index": 0, "start_ms": 1, "ms": 1});
        d["time"]["durations"].as_array_mut().unwrap().push(call);
    });
    types_alone("a request to a judge with no turn", &|d| {
        drop(remove(d, "/judge", 3))
    });
    types_alone("a turn of a request that was not to a judge", &|d| {
        d["models"][0]["capability"] = json!("fill");
    });
    types_alone("turns out of their order", &|d| {
        d["judge"].as_array_mut().unwrap().swap(0, 1)
    });
    types_alone("an exchange the run does not hold", &|d| {
        d["judge"][0]["exchange"] = json!("diagnostics/exchange-0900");
    });
    types_alone("a verdict before a turn", &|d| {
        d["judge"].as_array_mut().unwrap().swap(3, 4)
    });
    types_alone("two verdicts", &|d| {
        let verdict = d["judge"][4].clone();
        d["judge"].as_array_mut().unwrap().push(verdict);
    });
    types_alone("a verdict of a run that did not complete", &|d| {
        d["outcome"]["status"] = json!("cancelled");
    });
    // An exchange that could not be kept is null, and that is a record.
    let (bytes, document) = changed(&record, &|d| d["judge"][0]["exchange"] = json!(null));
    validate(&schema(), &document).unwrap();
    assert_eq!(
        RunRecord::from_bytes(&bytes),
        Err(RecordError::NotCanonical)
    );
}
