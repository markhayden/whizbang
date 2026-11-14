//! What a run asks of its capabilities (ADR 0009): an action is handed the backend its
//! call resolved and no other; each request it sends is in its run's record, with the
//! model that answered and how long it took; each exchange with a judge is kept whole
//! as a diagnostic and summarised in the record; an opaque part's body is never
//! stored; and a verdict is written with a run that completed, and with no other.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Value as Json, json};
use whizbang_core::artifact::{self, Artifact, ArtifactType, ImageRole};
use whizbang_core::capability::written::EXCHANGE;
use whizbang_core::capability::{
    Controls, Declaration, DeclarationError, EXCHANGES_MAX_BYTES, Excluded, Fault, FaultKind,
    FillRequest, Judge, JudgeRequest, MODEL_NAME_LIMIT, Message, Opaque, Part, Picture,
    REQUESTS_LIMIT, Role, Roster, StopReason, Support, ToolCall, Turn, Usage,
};
use whizbang_core::contract::ModelIdentity;
use whizbang_core::contract::record::{
    Decision, Judged, MODELS_LIMIT, RequestEnded, RunRecord, Status, Stop, Timed,
};
use whizbang_core::contract::{
    ARGUMENTS_TEXT_MAX_BYTES, Action, ActionDescriptor, Argument, Backends, Call, Capabilities,
    Capability, Clock, Context, Count, DIAGNOSTICS_LIMIT, EXCHANGE_PREFIX, Engine, Executor,
    Failure, INPUTS_LIMIT, InputPort, Kind, NoBackends, PreparedCall, Produced, RECORD_MAX_BYTES,
    RecordError, Registry, Requirement, RunId, Secrets, ValueType, Verdict,
};
use whizbang_core::contract::{MESSAGE_LIMIT, Value};
use whizbang_core::identity::{CanonicalPixels, Digest};
use whizbang_core::store::{Kind as Stored, Reader};
use whizbang_testkit::barrier::Barriers;
use whizbang_testkit::capabilities::{FakeFiller, FakeJudge, Paint, roster};
use whizbang_testkit::schema::validate;
use whizbang_testkit::threads;

use super::commit::World;
use super::record::schema;
use super::redact::made_up_key;
use crate::artifact::support::{image_of, photo};
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
        .argument(Argument::optional("judge", "A judge.", ValueType::Text))
        .argument(Argument::optional("note", "A note.", ValueType::Text));
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

    // The action was handed the turn's text as it came. A fault's sentence is cut, so
    // it is handed that with the key taken out first.
    let handed = handed.lock().unwrap().clone();
    assert!(
        handed[0].contains(&key) && handed[1] == "refused ?redacted?",
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
    assert_eq!(provider_id.as_deref(), Some("?redacted?"));
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

// --- a provider's words, and what is cut ------------------------------------------------------

/// A made-up key that no cleaning changes, and one that holds marks cleaning replaces.
fn plain_key() -> String {
    format!("{}{}", "zqHEADofKEY", "0123456789abcdTAIL")
}

fn marked_key() -> String {
    format!("{}{}", "zqSLASH/part+two=", "KEY9876543210")
}

#[test]
fn a_secret_that_a_cut_of_a_providers_words_goes_through_is_stored_nowhere_and_told_to_no_action() {
    // A provider's id is cut at 128 characters, its word for a stop reason at 64 and a
    // fault's sentence at 240, and each is cleaned. A key lies across each cut, and a
    // key with marks that cleaning replaces lies whole in a fault. The handle takes
    // the keys out while the words are whole: no part of one is in anything the run
    // stores, nor in the turn and the faults the action is handed.
    let world = World::new();
    let (key, marked) = (plain_key(), marked_key());
    let secrets = Secrets::new([key.clone(), marked.clone()]).unwrap();
    let id = format!("{}{key}", "r".repeat(120));
    let word = format!("{}{key}", "o".repeat(56));
    let parts = vec![Part::Text("ok".to_owned())];
    let turn = Turn::new(parts, StopReason::Other(word), None, Some(&id)).unwrap();
    // Before a handle has looked, the cut has left the head of the key in each.
    assert!(turn.request_id().unwrap().ends_with(&key[..8]));
    let cut = Fault::new(FaultKind::Rejected, &format!("{} {key}", "x".repeat(229)));
    assert!(cut.sentence().ends_with(&key[..10]));
    let changed = Fault::new(
        FaultKind::Rejected,
        &format!("the key {marked} was refused"),
    );
    assert!(
        changed
            .sentence()
            .contains("zqSLASH?part+two?KEY9876543210")
    );
    let judge = Arc::new(FakeJudge::scripted(vec![Ok(turn), Err(cut), Err(changed)]));
    let told: Noted<String> = Noted::default();
    let asks = {
        let told = told.clone();
        action("asks", judging(), move |_, context| {
            let judge = context.capabilities().judge().unwrap();
            let turn = judge.send(&asking("well?", 64)).unwrap();
            let mut told = told.lock().unwrap();
            told.push(format!("{:?} {:?}", turn.stop(), turn.request_id()));
            for _ in 0..2 {
                let fault = judge.send(&asking("well?", 64)).unwrap_err();
                told.push(format!("{fault} {fault:?}"));
            }
            Ok(Produced::nothing())
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
    let parts_of_a_key: [&[u8]; 4] = [b"zqHEAD", b"zqSLASH", b"KEY9876543210", b"TAIL"];
    let told = told.lock().unwrap().clone();
    assert_eq!(told.len(), 3);
    for said in &told {
        for part in parts_of_a_key {
            assert!(!holds(said.as_bytes(), part), "the action was told {said}");
        }
    }
    // What it was told instead: the words with the marker where the key stood, cleaned.
    assert!(
        told[0].contains(&format!("{}?redacte", "o".repeat(56))),
        "{}",
        told[0]
    );
    assert!(
        told[2].contains("the key ?redacted? was refused"),
        "{}",
        told[2]
    );
    let files = files_under(&world.root());
    assert!(files.len() > 5);
    for (path, bytes) in &files {
        for part in parts_of_a_key {
            assert!(!holds(bytes, part), "{path:?} holds a part of a key");
        }
    }
    let (record, names) = stored(&world, finished.run());
    let Judged::Turn { provider_id, .. } = &record.judge()[0] else {
        panic!("a turn");
    };
    assert_eq!(
        provider_id.as_deref(),
        Some(format!("{}?redacte", "r".repeat(120)).as_str())
    );
    let second: Json = serde_json::from_slice(&names["diagnostics/exchange-0002"]).unwrap();
    let sentence = second["answer"]["fault"]["sentence"].as_str().unwrap();
    assert_eq!(sentence, format!("{} ?redacted?", "x".repeat(229)));
}

#[test]
fn a_fault_of_any_capability_is_told_to_an_action_with_the_secrets_taken_out_whole() {
    // A filler's fault whose sentence the cut goes through a key of. The action, which
    // could copy the sentence into its own failure, is handed none of the key.
    let world = World::new();
    let key = plain_key();
    let fault = Fault::new(
        FaultKind::Unavailable,
        &format!("{} {key}", "x".repeat(229)),
    );
    let filler = Arc::new(FakeFiller::new(Paint::Fails(fault)));
    let asks = action(
        "asks",
        vec![Requirement::new(Capability::Fill)],
        |_, context| {
            let fault = context
                .capabilities()
                .fill()
                .unwrap()
                .fill(&fill_request())
                .unwrap_err();
            Err(Failure::error("fill", fault.sentence()))
        },
    );
    let executor = engine(
        &world,
        roster(None, Some(filler.as_ref()), None),
        Capabilities::none().with_fill(filler.clone()),
        vec![asks],
    )
    .with_secrets(Secrets::new([key.clone()]).unwrap());
    let finished = executor.submit(&Call::new("asks")).unwrap();
    let (record, names) = stored(&world, finished.run());
    let message = &record.outcome().error.as_ref().unwrap().message;
    assert_eq!(*message, format!("{} ?redacted?", "x".repeat(229)));
    assert!(!holds(&names["record"], b"zqHEAD"));
}

#[test]
fn words_that_cleaning_turns_into_a_secret_or_that_are_too_long_to_search_are_not_kept() {
    // Cleaning replaces a mark by `?`. A secret that is what the cleaned words read
    // as is found after the cleaning, and then none of the words is kept. Words over
    // a mebibyte are not searched, and none of them is kept either.
    let world = World::new();
    let secret = "pass?word?99".to_owned();
    let secrets = Secrets::new([secret.clone()]).unwrap();
    let becomes = Fault::new(FaultKind::Rejected, "it said pass/word=99 to me");
    assert!(becomes.sentence().contains(&secret));
    let long = Fault::new(FaultKind::Rejected, &"a long reason ".repeat(80_000));
    let judge = Arc::new(FakeJudge::scripted(vec![Err(becomes), Err(long)]));
    let told: Noted<String> = Noted::default();
    let asks = {
        let told = told.clone();
        action("asks", judging(), move |_, context| {
            let judge = context.capabilities().judge().unwrap();
            for _ in 0..2 {
                let fault = judge.send(&asking("well?", 64)).unwrap_err();
                told.lock().unwrap().push(fault.sentence().to_owned());
            }
            Ok(Produced::nothing())
        })
    };
    let executor = engine(
        &world,
        roster(None, None, Some(judge.as_ref())),
        Capabilities::none().with_judge(judge.clone()),
        vec![asks],
    )
    .with_secrets(secrets);
    executor.submit(&Call::new("asks")).unwrap();
    assert_eq!(*told.lock().unwrap(), ["the reason is not kept"; 2]);
    for (path, bytes) in files_under(&world.root()) {
        assert!(!holds(&bytes, secret.as_bytes()), "{path:?}");
    }
}

#[test]
fn an_id_that_redaction_lengthens_is_cut_again_and_its_record_is_read_back() {
    // The marker is ten bytes and a secret may be eight: an id of 128 characters that
    // ends in one is longer once it is redacted. It is cut after that, so the record
    // holds 128 characters, the reader takes it, and a status is answered.
    let world = World::new();
    let key = format!("{}{}", "zq7K", "m2Xw");
    let secrets = Secrets::new([key.clone()]).unwrap();
    let id = format!("{}{key}", "r".repeat(120));
    let parts = vec![Part::Text("ok".to_owned())];
    let turn = Turn::new(parts, StopReason::EndTurn, None, Some(&id)).unwrap();
    let judge = Arc::new(FakeJudge::scripted(vec![Ok(turn)]));
    let asks = action("asks", judging(), |_, context| {
        let judge = context.capabilities().judge().unwrap();
        let turn = judge.send(&asking("well?", 64)).unwrap();
        assert_eq!(turn.request_id().unwrap().chars().count(), 128);
        Ok(Produced::nothing())
    });
    let executor = engine(
        &world,
        roster(None, None, Some(judge.as_ref())),
        Capabilities::none().with_judge(judge.clone()),
        vec![asks],
    )
    .with_secrets(secrets);
    let finished = executor.submit(&Call::new("asks")).unwrap();
    let (record, _) = stored(&world, finished.run());
    let Judged::Turn { provider_id, .. } = &record.judge()[0] else {
        panic!("a turn");
    };
    let expected = format!("{}?redacte", "r".repeat(120));
    assert_eq!(provider_id.as_deref(), Some(expected.as_str()));
    executor.status(finished.run()).unwrap();
}

#[test]
fn a_text_whose_cut_and_three_dots_complete_a_secret_is_kept_without_the_dots() {
    // The secret ends in three dots. The reason is cut just after the rest of it, so
    // the dots that mark the cut complete it. It is not kept that way: the dots are
    // left off, what is kept is within what a reader takes, and the record is read.
    let world = World::new();
    let secret = "abcde...".to_owned();
    let secrets = Secrets::new([secret.clone()]).unwrap();
    let concludes = action("concludes", Vec::new(), |_, _| {
        let why = format!("{}abcde and more", "x".repeat(MESSAGE_LIMIT - 5));
        Ok(Produced::nothing().with_verdict(Verdict::new(Decision::Pass, &why)))
    });
    let executor =
        engine(&world, Roster::new(), Capabilities::none(), vec![concludes]).with_secrets(secrets);
    let finished = executor.submit(&Call::new("concludes")).unwrap();
    let (record, _) = stored(&world, finished.run());
    let (_, why) = record.verdict().unwrap();
    assert_eq!(why, format!("{}abcde", "x".repeat(MESSAGE_LIMIT - 5)));
    for (path, bytes) in files_under(&world.root()) {
        assert!(!holds(&bytes, secret.as_bytes()), "{path:?}");
    }
}

// --- a model's names, and how large a record can be -------------------------------------------

#[test]
fn the_names_of_a_backend_a_model_and_a_version_are_bounded_where_they_are_declared() {
    let declared = |backend: &str, model: &str, version: &str| {
        Declaration::new(ModelIdentity::new(backend, model, version), &[])
    };
    let longest = "m".repeat(MODEL_NAME_LIMIT);
    declared(&longest, &longest, &longest).unwrap();
    declared("fal.ai", "sam-3/large@2026", "v1.0+cu121").unwrap();
    let long = "m".repeat(MODEL_NAME_LIMIT + 1);
    for wrong in [
        long.as_str(),
        "",
        "two words",
        "caf\u{e9}",
        "a\"b",
        "a\\b",
        "a\nb",
    ] {
        let model = |what| Err(DeclarationError::Model { what });
        assert_eq!(declared(wrong, "m", "1"), model("backend"), "{wrong:?}");
        assert_eq!(declared("b", wrong, "1"), model("model"), "{wrong:?}");
        assert_eq!(declared("b", "m", wrong), model("version"), "{wrong:?}");
    }
    // The refusal does not show the name.
    let refusal = declared("b", "se cret", "1").unwrap_err();
    assert!(!format!("{refusal} {refusal:?}").contains("cret"));
}

/// A judge of this model, backend and version, whose script is empty.
struct Named(Declaration);

impl Judge for Named {
    fn declaration(&self) -> &Declaration {
        &self.0
    }

    fn send(&self, _: &JudgeRequest, _: &whizbang_core::capability::Bounds) -> Result<Turn, Fault> {
        Err(Fault::new(FaultKind::Unavailable, "nobody is there"))
    }
}

#[test]
fn a_secret_and_a_long_text_in_a_models_names_are_redacted_and_then_cut_in_the_record() {
    // Each name is as long as one may be and ends in a key of eight bytes. Redacted,
    // each is two characters longer; it is cut after that. The record is read back,
    // and holds no part of the key that is not a part of the marker.
    let world = World::new();
    let key = format!("{}{}", "zq7K", "m2Xw");
    let secrets = Secrets::new([key.clone()]).unwrap();
    let name = |first: char| format!("{}{key}", first.to_string().repeat(MODEL_NAME_LIMIT - 8));
    let model = ModelIdentity::new(&name('b'), &name('m'), &name('v'));
    let features = ["image", "tool"];
    let judge = Arc::new(Named(Declaration::new(model, &features).unwrap()));
    let asks = action("asks", judging(), |_, context| {
        let judge = context.capabilities().judge().unwrap();
        judge.send(&asking("well?", 64)).unwrap_err();
        Ok(Produced::nothing())
    });
    let executor = engine(
        &world,
        roster(None, None, Some(judge.as_ref())),
        Capabilities::none().with_judge(judge.clone()),
        vec![asks],
    )
    .with_secrets(secrets);
    let finished = executor.submit(&Call::new("asks")).unwrap();
    let (record, names) = stored(&world, finished.run());
    let kept = &record.models()[0];
    let cut = |first: char| format!("{}[redacte", first.to_string().repeat(MODEL_NAME_LIMIT - 8));
    assert_eq!(
        (&kept.backend, &kept.model, &kept.version),
        (&cut('b'), &cut('m'), &cut('v'))
    );
    assert!(!holds(&names["record"], key.as_bytes()));
}

#[test]
fn a_record_of_as_many_requests_as_a_run_sends_is_under_the_limit_by_arithmetic() {
    // Twenty-four requests to a judge whose three names are each as long as one may
    // be, and whose ids are as long as one is kept. What one request adds to the
    // record is measured, and held to the figure ADR 0009 does its sum with.
    let world = World::new();
    let longest = |first: &str| first.repeat(MODEL_NAME_LIMIT);
    let model = ModelIdentity::new(&longest("b"), &longest("m"), &longest("v"));
    let declaration = Declaration::new(model, &["image", "tool"]).unwrap();
    let id = "i".repeat(128);
    let turn = || {
        let parts = vec![Part::Text("ok".to_owned())];
        let usage = Usage::new(u64::MAX, u64::MAX).cached(u64::MAX, u64::MAX);
        Turn::new(parts, StopReason::MaxTokens, Some(usage), Some(&id)).unwrap()
    };
    let script = (0..FEW).map(|_| Ok(turn())).collect();
    let judge = Arc::new(FakeJudge::scripted(script).declared_as(declaration));
    let asks = action("asks", judging(), |call, context| {
        let judge = context.capabilities().judge().unwrap();
        let sends = call.arguments().iter().any(|(name, _)| name == "note");
        for _ in 0..if sends { FEW } else { 0 } {
            judge.send(&asking("well?", u32::MAX)).unwrap();
            judge.exclude_last(Excluded::Truncated);
        }
        Ok(Produced::nothing())
    });
    let executor = engine(
        &world,
        roster(None, None, Some(judge.as_ref())),
        Capabilities::none().with_judge(judge.clone()),
        vec![asks],
    );
    let bytes = |call: Call| {
        let finished = executor.submit(&call).unwrap();
        let (record, names) = stored(&world, finished.run());
        (record.models().len(), names["record"].len())
    };
    let (none, without) = bytes(Call::new("asks"));
    let (all, with) = bytes(Call::new("asks").argument("note", Value::text("x")));
    assert_eq!((none, all), (0, FEW));
    // A request's model, its duration, its turn and its exchange among the diagnostics.
    let each = (with - without).div_ceil(FEW);
    assert!((1000..=1211).contains(&each), "{each} bytes a request");
    // The sum of ADR 0009, with 1,211 bytes a request, which is what one takes when
    // its times have twenty digits and its indexes four: the requests, 256 steps of
    // 256 bytes, 256 refusals of 512 characters of six bytes each, and three texts of
    // 4,099 characters of six bytes each. A count with every field at the schema's
    // own maximum is 2,364,470. Either leaves 1.8 megabytes for what the executor
    // does not bound.
    let worst = 1211 * REQUESTS_LIMIT + 256 * 256 + 256 * (512 * 6 + 128) + 3 * 4099 * 6;
    assert_eq!(worst, 2_198_582);
    let apart = 2_364_470;
    assert!(worst.max(apart) as u64 + 1_800_000 < RECORD_MAX_BYTES);
}

#[test]
fn a_calls_text_arguments_are_bounded_where_the_call_comes_in() {
    // An argument's text is recorded as it was given, so what a call may give is
    // bounded: 64 KiB for its texts together. A call over that is refused before any
    // run exists, in a sentence that shows none of the text; a call at the bound
    // runs, and its record is read back though every byte of the text is one that
    // JSON writes as six.
    assert_eq!(ARGUMENTS_TEXT_MAX_BYTES, 64 * 1024);
    let world = World::new();
    let asks = action("asks", Vec::new(), |_, _| Ok(Produced::nothing()));
    let executor = engine(&world, NoBackends, Capabilities::none(), vec![asks]);
    let call = |judge: usize, note: usize| {
        Call::new("asks")
            .argument("judge", Value::text(&"\u{1}".repeat(judge)))
            .argument("note", Value::text(&"\u{1}".repeat(note)))
    };
    for over in [
        call(0, ARGUMENTS_TEXT_MAX_BYTES + 1),
        call(1, ARGUMENTS_TEXT_MAX_BYTES),
    ] {
        for refused in [executor.prepare(&over).err(), executor.submit(&over).err()] {
            assert_eq!(
                refused.unwrap().to_string(),
                "not a call of asks: its text arguments hold 65537 bytes together, and a \
                 call's hold 65536 at most"
            );
        }
    }
    let reader = Reader::open(&world.root(), world.fs.clone()).unwrap();
    assert!(reader.snapshot().unwrap().runs().is_empty());
    let finished = executor.submit(&call(1, ARGUMENTS_TEXT_MAX_BYTES - 1));
    let (_, names) = stored(&world, finished.unwrap().run());
    assert!(names["record"].len() > 6 * ARGUMENTS_TEXT_MAX_BYTES);
}

#[test]
fn the_exchanges_the_executor_keeps_are_not_counted_among_a_runs_diagnostics() {
    // A run's action writes 1,024 diagnostics at most. The exchanges with a judge are
    // the executor's, bounded with the run's requests, and take none of those: a run
    // that sent requests still writes as many, and one that wrote as many still has
    // its next exchange kept.
    let world = World::new();
    let judge = Arc::new(FakeJudge::scripted(vec![Ok(said("yes")), Ok(said("yes"))]));
    let asks = action("asks", judging(), |_, context| {
        let judge = context.capabilities().judge().unwrap();
        judge.send(&asking("well?", 64)).unwrap();
        for at in 0..DIAGNOSTICS_LIMIT {
            let written = context.diagnostics().write(&format!("d{at}"), b"x");
            written.unwrap();
        }
        assert!(context.diagnostics().write("one-more", b"x").is_err());
        judge.send(&asking("and now?", 64)).unwrap();
        Ok(Produced::nothing())
    });
    let executor = engine(
        &world,
        roster(None, None, Some(judge.as_ref())),
        Capabilities::none().with_judge(judge.clone()),
        vec![asks],
    );
    let finished = executor.submit(&Call::new("asks")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    let (record, names) = stored(&world, finished.run());
    // The journal, two exchanges, and what the action wrote.
    assert_eq!(record.diagnostics().len(), 3 + DIAGNOSTICS_LIMIT);
    assert!(names.contains_key("diagnostics/exchange-0002"));
    assert!(!names.contains_key("diagnostics/one-more"));
}

#[test]
fn everything_a_record_lists_but_outputs_is_bounded_and_the_sum_is_under_the_limit() {
    // What the executor does not fill itself, at its bounds: the texts of a call at
    // 64 KiB of bytes that JSON writes as six, 1,024 inputs and 1,024 diagnostics
    // under the longest names. What they add to a record is measured, and with the
    // most the executor's own lists can come to (the test above) it is under the
    // limit. So no record is over it but one of a run that handed back outputs, and
    // such a run commits none.
    let world = World::new();
    let image = image_of(photo(), ImageRole::Picture);
    {
        let store = world.store();
        let mut publication = store.begin().unwrap();
        artifact::put(&mut publication, &Artifact::from(image.clone())).unwrap();
        publication.publish().unwrap();
    }
    let draft = ActionDescriptor::draft("full", "1.0.0", Kind::Primitive, "A test action.")
        .argument(Argument::optional("note", "A note.", ValueType::Text))
        .input(InputPort::variadic(
            "images",
            ArtifactType::Image,
            "Any number of images.",
            Count::at_least(0),
        ));
    let body: Body = Box::new(|call, context| {
        for at in 0..call.inputs().len() {
            let name = format!("{at:04}{}", "d".repeat(60));
            context.diagnostics().write(&name, b"x").unwrap();
        }
        Ok(Produced::nothing())
    });
    let full: Arc<dyn Action> = Arc::new(Scripted(draft.build().unwrap(), body));
    let executor = engine(&world, NoBackends, Capabilities::none(), vec![full]);
    let bytes = |call: Call| {
        let finished = executor.submit(&call).unwrap();
        stored(&world, finished.run()).1["record"].len()
    };
    let empty = bytes(Call::new("full"));
    let mut call = Call::new("full").argument(
        "note",
        Value::text(&"\u{1}".repeat(ARGUMENTS_TEXT_MAX_BYTES)),
    );
    for _ in 0..INPUTS_LIMIT {
        call = call.input("images", image.id());
    }
    let added = bytes(call) - empty;
    // 393,216 for the texts, and about 383 for an input and a diagnostic together.
    assert!((780_000..=800_000).contains(&added), "{added}");
    // The sum of ADR 0008 is done with the longest an input and a diagnostic can be
    // by the schema, not with these: a port's name of 64 characters, and a length of
    // twenty digits. That is about 480,000 for the two lists, where 392,000 was
    // measured here; with the texts and the executor's own lists, 3.26 megabytes.
    let (texts, lists, executors_own) = (6 * ARGUMENTS_TEXT_MAX_BYTES, 480_000, 2_364_470);
    assert!(added - texts < lists);
    assert!((texts + lists + executors_own + empty) as u64 + 900_000 < RECORD_MAX_BYTES);
}

#[test]
fn a_record_that_would_be_over_the_limit_is_not_published_as_it_is() {
    // Requests that take the record over the limit, which here is lowered through
    // the executor's seam for tests: with what a call and a run may hold bounded
    // where it comes in, the sum of what a record lists is under the real one. The
    // record that would not be read back is not written: the run ends as an error
    // of the type `record`, and its record, which leaves the requests out and says
    // how many there were, is read back.
    let world = World::new();
    let judge = Arc::new(FakeJudge::scripted(Vec::new()));
    let asks = action("asks", judging(), |_, context| {
        let judge = context.capabilities().judge().unwrap();
        for _ in 0..FEW {
            judge.send(&asking("again", 64)).unwrap_err();
        }
        Ok(Produced::nothing().with_verdict(Verdict::new(Decision::Pass, "fine")))
    });
    let executor = engine(
        &world,
        roster(None, None, Some(judge.as_ref())),
        Capabilities::none().with_judge(judge.clone()),
        vec![asks],
    )
    .with_record_limit(9_000);
    let finished = executor.submit(&Call::new("asks")).unwrap();
    let (record, names) = stored(&world, finished.run());
    assert!(names["record"].len() < 9_000, "{}", names["record"].len());
    assert_eq!(record.outcome().status, Status::Error);
    let error = record.outcome().error.as_ref().unwrap();
    assert_eq!(error.kind, "record");
    assert!(
        error
            .message
            .starts_with("the run sent 24 capability requests, and"),
        "{}",
        error.message
    );
    assert!(record.models().is_empty() && record.judge().is_empty());
    assert!(record.time().durations.is_empty());
    // The exchanges were kept, and the record still lists them.
    assert!(names.contains_key("diagnostics/exchange-0024"));
    executor.status(finished.run()).unwrap();
}

// --- how much a run stores of its exchanges ---------------------------------------------------

#[test]
fn a_request_whose_exchange_the_run_has_no_room_for_is_not_sent_and_the_record_says_so() {
    // The run may store 4,000 bytes of exchanges. Each request here takes about 1,500
    // as it is written: two are sent, and the third is refused before it is sent.
    assert_eq!(EXCHANGES_MAX_BYTES, 256 << 20);
    let world = World::new();
    let judge = Arc::new(FakeJudge::scripted(vec![Ok(said("yes")), Ok(said("yes"))]));
    let asks = action("asks", judging(), |_, context| {
        let judge = context.capabilities().judge().unwrap();
        let long = "q".repeat(1200);
        judge.send(&asking(&long, 64)).unwrap();
        judge.send(&asking(&long, 64)).unwrap();
        let fault = judge.send(&asking(&long, 64)).unwrap_err();
        assert_eq!(fault.kind(), FaultKind::TooLarge);
        assert!(fault.sentence().contains("the run may store"), "{fault}");
        Ok(Produced::nothing())
    });
    let executor = engine(
        &world,
        roster(None, None, Some(judge.as_ref())),
        Capabilities::none().with_judge(judge.clone()),
        vec![asks],
    )
    .with_exchanges_budget(4000);
    let finished = executor.submit(&Call::new("asks")).unwrap();
    assert_eq!(judge.requests(), 2);
    let (record, names) = stored(&world, finished.run());
    assert_eq!(record.models()[2].ended, RequestEnded::TooLarge);
    let Judged::Turn {
        exchange,
        stop,
        messages,
        ..
    } = &record.judge()[2]
    else {
        panic!("a turn");
    };
    assert_eq!((exchange, stop, *messages), (&None, &None, 1));
    let kept: usize = names
        .iter()
        .filter(|(name, _)| name.contains("exchange"))
        .map(|(_, b)| b.len())
        .sum();
    assert!(
        kept <= 4000 && names.contains_key("diagnostics/exchange-0002"),
        "{kept}"
    );
    assert!(!names.contains_key("diagnostics/exchange-0003"));
}

#[test]
fn two_requests_at_once_cannot_both_take_the_last_of_what_a_run_may_store() {
    // The run may store 500,000 bytes. Two requests are open at once, held by the
    // judge at a barrier, and each is answered with 300,000 bytes. Room for an
    // exchange is taken in one step with the look at what is left, so one of the two
    // is kept whole and the other as its sizes, whatever the order: together they are
    // never over what the run may store. Both actions' threads are handed their turn.
    //
    // What this catches is room that is not taken before an exchange is stored. It
    // does not catch a look and a taking that were split into two steps under two
    // holds of the lock: the two are a few instructions in one function
    // (`Sent::reserve`), nothing can be stopped between them without a seam inside
    // the lock, and none was added. That they are one step is read in the code.
    let world = World::new();
    let barriers = Barriers::new();
    let answer = "a".repeat(300_000);
    let script = vec![Ok(said(&answer)), Ok(said(&answer))];
    let judge = Arc::new(FakeJudge::scripted(script).stopping_at(&barriers, "judge"));
    let asks = {
        let barriers = barriers.clone();
        action("asks", judging(), move |_, context| {
            let judge = context.capabilities().judge().unwrap();
            threads::scope(|scope| {
                for _ in 0..2 {
                    scope.spawn(|| {
                        let turn = judge.send(&asking("well?", 64)).unwrap();
                        assert_eq!(turn.text_bytes(), 300_000);
                    });
                }
                barriers.arrived("judge", 2);
                barriers.open("judge");
            });
            Ok(Produced::nothing())
        })
    };
    let executor = engine(
        &world,
        roster(None, None, Some(judge.as_ref())),
        Capabilities::none().with_judge(judge.clone()),
        vec![asks],
    )
    .with_exchanges_budget(500_000);
    let finished = executor.submit(&Call::new("asks")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    let (_, names) = stored(&world, finished.run());
    let exchanges = names.iter().filter(|(name, _)| name.contains("exchange"));
    let mut sizes: Vec<usize> = exchanges.map(|(_, bytes)| bytes.len()).collect();
    sizes.sort_unstable();
    assert_eq!(sizes.len(), 2);
    assert!(sizes.iter().sum::<usize>() <= 500_000, "{sizes:?}");
    assert!(sizes[0] < 1000 && sizes[1] > 300_000, "{sizes:?}");
}

/// A judge whose first call panics, and that answers after that.
struct PanicsOnce(Declaration, AtomicU64);

impl Judge for PanicsOnce {
    fn declaration(&self) -> &Declaration {
        &self.0
    }

    fn send(&self, _: &JudgeRequest, _: &whizbang_core::capability::Bounds) -> Result<Turn, Fault> {
        if self.1.fetch_add(1, Ordering::SeqCst) == 0 {
            panic!("the backend fell over");
        }
        Ok(said("ok"))
    }
}

#[test]
fn a_request_that_unwound_gives_back_the_room_it_took() {
    // A request takes room for what it sends before it sends. The backend panics, and
    // the action catches the panic and asks again. The run may store 2,000 bytes and
    // each request takes about 1,300 before it is sent: the second is sent only
    // because the first, which stored nothing, gave its room back.
    let world = World::new();
    let fake = FakeJudge::scripted(Vec::new());
    let judge = Arc::new(PanicsOnce(fake.declaration().clone(), AtomicU64::new(0)));
    let asks = action("asks", judging(), |_, context| {
        let judge = context.capabilities().judge().unwrap();
        let send = || judge.send(&asking("well?", 64));
        let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(send));
        assert!(unwound.is_err());
        send().unwrap();
        Ok(Produced::nothing())
    });
    let executor = engine(
        &world,
        roster(None, None, Some(judge.as_ref())),
        Capabilities::none().with_judge(judge.clone()),
        vec![asks],
    )
    .with_exchanges_budget(2000);
    let finished = executor.submit(&Call::new("asks")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    let (record, names) = stored(&world, finished.run());
    let ended: Vec<_> = record.models().iter().map(|model| model.ended).collect();
    assert_eq!(ended, [RequestEnded::Unwound, RequestEnded::Answered]);
    assert!(names.contains_key("diagnostics/exchange-0002"));
    assert!(!names.contains_key("diagnostics/exchange-0001"));
}

#[test]
fn a_request_whose_exchange_could_not_be_stored_gives_back_the_room_it_took() {
    // What is sent holds a key of eight bytes a thousand times. As it is written it
    // fits what the run may store, so it is sent; with each key replaced by a marker
    // of ten it is two thousand bytes longer and fits neither whole nor with its turn
    // not written, so nothing of it is stored. Its room is given back: a small
    // request after it is sent, which it would not be were the room still held.
    let key = format!("{}{}", "zq7K", "m2Xw");
    let question = key.repeat(1000);
    let ask = |budget: u64| {
        let world = World::new();
        let judge = Arc::new(FakeJudge::scripted(vec![Ok(said("ok")), Ok(said("ok"))]));
        let asks = {
            let question = question.clone();
            action("asks", judging(), move |_, context| {
                let judge = context.capabilities().judge().unwrap();
                judge.send(&asking(&question, 64)).unwrap();
                let second = judge.send(&asking("well?", 64));
                Ok(Produced::nothing().with_verdict(Verdict::new(
                    Decision::Pass,
                    if second.is_ok() { "sent" } else { "not sent" },
                )))
            })
        };
        let executor = engine(
            &world,
            roster(None, None, Some(judge.as_ref())),
            Capabilities::none().with_judge(judge.clone()),
            vec![asks],
        )
        .with_exchanges_budget(budget)
        .with_secrets(Secrets::new([key.clone()]).unwrap());
        let finished = executor.submit(&Call::new("asks")).unwrap();
        let (record, names) = stored(&world, finished.run());
        let first = names.get("diagnostics/exchange-0001").map(Vec::len);
        (
            record.verdict().unwrap().1.to_owned(),
            first,
            judge.requests(),
        )
    };
    // With room to spare the first exchange is stored: that is how long it is.
    let (said, stored_whole, sent) = ask(1 << 20);
    assert_eq!((said.as_str(), sent), ("sent", 2));
    let stored_whole = stored_whole.unwrap() as u64;
    // Room for what is written, and not for what would be stored.
    let (said, first, sent) = ask(stored_whole - 620);
    assert_eq!(first, None, "nothing of the first exchange was stored");
    assert_eq!((said.as_str(), sent), ("sent", 2));
}

#[test]
fn an_answer_the_run_has_no_room_for_is_kept_as_its_sizes_and_the_action_still_has_it() {
    // There is room for what is sent and not for the turn that comes back. The action
    // is handed the turn; what is stored says how large it was, with its digest.
    let world = World::new();
    let answer = "a".repeat(5000);
    let judge = Arc::new(FakeJudge::scripted(vec![Ok(said(&answer))]));
    let asks = {
        let answer = answer.clone();
        action("asks", judging(), move |_, context| {
            let judge = context.capabilities().judge().unwrap();
            let turn = judge.send(&asking("well?", 64)).unwrap();
            assert_eq!(turn.texts().next(), Some(answer.as_str()));
            Ok(Produced::nothing())
        })
    };
    let executor = engine(
        &world,
        roster(None, None, Some(judge.as_ref())),
        Capabilities::none().with_judge(judge.clone()),
        vec![asks],
    )
    .with_exchanges_budget(3000);
    let finished = executor.submit(&Call::new("asks")).unwrap();
    let (record, names) = stored(&world, finished.run());
    assert_eq!(record.models()[0].ended, RequestEnded::Answered);
    let stored: Json = serde_json::from_slice(&names["diagnostics/exchange-0001"]).unwrap();
    let kept = &stored["answer"]["turn_withheld"];
    assert_eq!(
        (kept["parts"].as_u64(), kept["bytes"].as_u64()),
        (Some(1), Some(5000))
    );
    assert_eq!(
        kept["digest"],
        json!(Digest::of_bytes(answer.as_bytes()).to_string())
    );
    assert_eq!(kept["stop"], "end_turn");
    assert!(names["diagnostics/exchange-0001"].len() < 1000);
}

#[test]
fn a_turn_over_the_limit_its_handle_was_given_is_a_fault_and_is_stored_as_its_sizes() {
    let world = World::new();
    let judge = Arc::new(FakeJudge::scripted(vec![
        Ok(said(&"t".repeat(300))),
        Ok(said("ok")),
    ]));
    let asks = action("asks", judging(), |_, context| {
        let judge = context.capabilities().judge().unwrap().clone();
        // A limit is only narrowed.
        let judge = judge.turns_within(200).turns_within(100_000);
        let fault = judge.send(&asking("well?", 64)).unwrap_err();
        assert_eq!(fault.kind(), FaultKind::TooLarge);
        let said = "the turn holds 300 bytes of text, and at most 200 are taken";
        assert_eq!(fault.sentence(), said);
        judge.send(&asking("well?", 64)).unwrap();
        Ok(Produced::nothing())
    });
    let executor = engine(
        &world,
        roster(None, None, Some(judge.as_ref())),
        Capabilities::none().with_judge(judge.clone()),
        vec![asks],
    );
    let finished = executor.submit(&Call::new("asks")).unwrap();
    let (record, names) = stored(&world, finished.run());
    assert_eq!(record.models()[0].ended, RequestEnded::TooLarge);
    assert_eq!(record.models()[1].ended, RequestEnded::Answered);
    let stored: Json = serde_json::from_slice(&names["diagnostics/exchange-0001"]).unwrap();
    assert_eq!(
        stored["answer"]["turn_withheld"]["bytes"].as_u64(),
        Some(300)
    );
    assert!(!holds(
        &names["diagnostics/exchange-0001"],
        "t".repeat(40).as_bytes()
    ));
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
    let wrong: [(&str, Json); 19] = [
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
        ("/judge/0/max_output_tokens", json!(4_294_967_296_u64)),
        ("/models/0/backend", json!("b".repeat(129))),
        ("/models/0/version", json!("v".repeat(129))),
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
    // A name that is not ASCII is within the schema's count of characters.
    types_alone("a model's name that is not ASCII", &|d| {
        d["models"][0]["model"] = json!("caf\u{e9}");
    });
    // An exchange that could not be kept is null, and that is a record.
    let (bytes, document) = changed(&record, &|d| d["judge"][0]["exchange"] = json!(null));
    validate(&schema(), &document).unwrap();
    assert_eq!(
        RunRecord::from_bytes(&bytes),
        Err(RecordError::NotCanonical)
    );
}

#[test]
fn the_types_hold_a_record_to_as_many_requests_as_a_run_sends_whatever_else_agrees() {
    // One request more than a run sends, each with its duration, and none to a judge:
    // nothing else of the record disagrees, and it is refused for the count.
    let world = World::new();
    let (_, run) = judged(
        &world,
        Verdict::new(Decision::Pass, "fine"),
        Secrets::none(),
    );
    let (record, _) = stored(&world, &run);
    let over = |count: usize| {
        changed(&record, &|d| {
            let mut model = d["models"][0].clone();
            model["capability"] = json!("fill");
            d["models"] = json!(vec![model; count]);
            let lasted = |at| json!({"of": "request", "index": at, "start_ms": 1, "ms": 1});
            d["time"]["durations"] = (0..count).map(lasted).collect();
            d["judge"] = json!([]);
        })
        .0
    };
    let said = "more capability requests than a run may send";
    let refused = RunRecord::from_bytes(&over(MODELS_LIMIT + 1));
    assert_eq!(refused, Err(RecordError::Inconsistent(said.to_owned())));
    let taken = RunRecord::from_bytes(&over(MODELS_LIMIT));
    assert_eq!(
        taken,
        Err(RecordError::NotCanonical),
        "the count itself is allowed"
    );
    // The reader's own check of a name's length, whatever a schema says: a name of
    // 129 letters is refused by the check that says so, and one of 128 is not.
    let named = |letters: usize| {
        let change = |d: &mut Json| d["models"][0]["version"] = json!("v".repeat(letters));
        RunRecord::from_bytes(&changed(&record, &change).0)
    };
    let said = "a model's name is longer than one is kept, or is not ASCII";
    assert_eq!(named(129), Err(RecordError::Inconsistent(said.to_owned())));
    assert_eq!(named(128), Err(RecordError::NotCanonical));
}
