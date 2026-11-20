//! `serve::…`: the MCP server over stdio (plan 2C, task M1; ADR 0031), in process over
//! in-memory pipes: a client of the test's writes frames to the server's input and
//! reads every line of its output. The executor is the binary's runner; a judge is
//! scripted on loopback, and the pack's worker is the fakes' through the test kit's
//! guard. The server's thread is the test kit's, and every test ends the server's input
//! and waits for the server on its way out.

use std::io::{BufRead, BufReader, PipeReader, PipeWriter, Write};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::{Value as Json, json};
use whizbang::serving::Actions;
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Context, Engine, Executor, Failure, Kind, Outcome, Plan,
    Planning, PreparedCall, Produced, Registry, RunId, RunState, Value, ValueType,
};
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;
use whizbang_serve::{Door, Limits};
use whizbang_testkit::scratch::Scratch;

/// What the scripted action `waits` says, in order, before it waits: the second holds
/// an escape sequence and a line break, which the client is shown escaped.
const SAID: [&str; 2] = [
    "waiting for a cancel",
    "a \u{1b}[31mred\u{1b}[0m word\nand a second line",
];

/// `waits`: says [`SAID`], then waits until its run is cancelled or its deadline
/// passes, looking every 10 ms, 30 seconds at most.
struct Waits(ActionDescriptor);

impl Action for Waits {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }

    fn run(&self, _: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        for said in SAID {
            context.events().progress(said);
        }
        for _ in 0..3000 {
            if context.cancellation().is_cancelled() || context.deadline_passed() {
                return Err(Failure::error("stopped", "the run was told to stop"));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(Produced::nothing())
    }
}

fn waits() -> Arc<dyn Action> {
    let draft = ActionDescriptor::draft("waits", "1.0.0", Kind::Primitive, "A test action.");
    Arc::new(Waits(draft.build().unwrap()))
}

/// `deaf`: sleeps 600 ms and ignores its run's cancel and deadline.
struct Deaf(ActionDescriptor);

impl Action for Deaf {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }

    fn run(&self, _: &PreparedCall, _: &Context) -> Result<Produced, Failure> {
        std::thread::sleep(Duration::from_millis(600));
        Ok(Produced::nothing())
    }
}

/// A client of a server running on a thread of the test kit's.
pub struct Client {
    to: Option<PipeWriter>,
    from: BufReader<PipeReader>,
    server: Option<JoinHandle<std::io::Result<()>>>,
}

impl Client {
    pub fn new(executor: Arc<dyn Executor>, door: Arc<dyn Door>, limits: Limits) -> Client {
        let (input, to) = std::io::pipe().unwrap();
        let (from, output) = std::io::pipe().unwrap();
        let server = whizbang_testkit::threads::spawn(move || {
            whizbang_serve::serve(executor, door, limits, input, output)
        });
        Client {
            to: Some(to),
            from: BufReader::new(from),
            server: Some(server),
        }
    }

    pub fn line(&mut self, line: &str) {
        let to = self.to.as_mut().unwrap();
        to.write_all(line.as_bytes()).unwrap();
        to.write_all(b"\n").unwrap();
    }

    pub fn send(&mut self, message: &Json) {
        self.line(&message.to_string());
    }

    /// The next line the server wrote, which must be a message of the protocol.
    pub fn read(&mut self) -> Json {
        let mut line = String::new();
        let read = self.from.read_line(&mut line).unwrap();
        assert!(read > 0, "the server's output ended");
        protocol(&line)
    }

    /// The answer to the request of this id, the notifications before it kept.
    pub fn answer(&mut self, id: i64, notes: &mut Vec<Json>) -> Json {
        loop {
            let message = self.read();
            if message.get("id") == Some(&json!(id)) {
                return message;
            }
            assert!(message.get("method").is_some(), "{message}");
            notes.push(message);
        }
    }

    pub fn initialized(&mut self) {
        self.send(&json!({"jsonrpc": "2.0", "id": 0, "method": "initialize",
                          "params": {"protocolVersion": "2025-06-18", "capabilities": {},
                                     "clientInfo": {"name": "test", "version": "1"}}}));
        let answer = self.read();
        assert_eq!(answer["result"]["protocolVersion"], "2025-06-18");
        self.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    }

    pub fn call(&mut self, id: i64, tool: &str, arguments: Json, token: Option<&str>) {
        let mut params = json!({"name": tool, "arguments": arguments});
        if let Some(token) = token {
            params["_meta"] = json!({"progressToken": token});
        }
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": params}));
    }

    /// Ends the server's input, and waits for the server: every line it wrote after
    /// what was read, each a message of the protocol.
    pub fn close(mut self) -> Vec<Json> {
        drop(self.to.take());
        let mut rest = Vec::new();
        for line in (&mut self.from).lines() {
            rest.push(protocol(&line.unwrap()));
        }
        let served = self.server.take().unwrap().join().unwrap();
        served.unwrap();
        rest
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        drop(self.to.take());
        if let Some(server) = self.server.take() {
            let _ = server.join();
        }
    }
}

/// A line of the server's output, held to the protocol: a JSON-RPC 2.0 message,
/// an answer to a request or a notification.
pub fn protocol(line: &str) -> Json {
    let message: Json = serde_json::from_str(line.trim_end_matches('\n'))
        .unwrap_or_else(|_| panic!("not a message of the protocol: {line:?}"));
    assert_eq!(message["jsonrpc"], "2.0", "{line}");
    let answers = message.get("id").is_some()
        && (message.get("result").is_some() != message.get("error").is_some());
    let notes = message.get("id").is_none() && message["method"].is_string();
    assert!(answers || notes, "{line}");
    message
}

/// The text of a tool's result, and whether it is an error.
fn result_text(answer: &Json) -> (String, bool) {
    let result = &answer["result"];
    let text = result["content"][0]["text"].as_str().unwrap().to_owned();
    (text, result["isError"].as_bool().unwrap())
}

/// A store in a scratch folder, and the binary's runner over it with `waits` beside
/// the build's actions; and the door over the same actions.
fn waiting(label: &str) -> (Scratch, Arc<Engine>, Arc<Actions>) {
    let dir = Scratch::new(label);
    let root = dir.path().join("store");
    let store = Arc::new(Store::open(&root, Arc::new(RealFs)).unwrap());
    let mut registry: Registry = whizbang::wiring::registry().unwrap();
    registry.register(waits()).unwrap();
    let draft = ActionDescriptor::draft("deaf", "1.0.0", Kind::Primitive, "A test action.");
    registry
        .register(Arc::new(Deaf(draft.build().unwrap())))
        .unwrap();
    let descriptors = registry.descriptors().cloned().collect();
    let engine = crate::runner(registry, store);
    (
        dir,
        Arc::new(engine),
        Arc::new(Actions::new(descriptors, root, None)),
    )
}

/// The run a call started, from its first progress.
fn started(notes: &[Json]) -> RunId {
    let said = notes[0]["params"]["message"].as_str().unwrap();
    let id = said
        .strip_prefix("run ")
        .unwrap()
        .strip_suffix(" started")
        .unwrap();
    RunId::parse(id).unwrap()
}

/// Reads notifications until one says this, keeping them.
fn until_said(client: &mut Client, said: &str, notes: &mut Vec<Json>) {
    for _ in 0..16 {
        let note = client.read();
        let found = note["params"]["message"] == said;
        notes.push(note);
        if found {
            return;
        }
    }
    panic!("never said: {said}");
}

fn ended(executor: &Engine, run: &RunId) -> Outcome {
    match executor.wait(run).unwrap().state() {
        RunState::Ended(outcome) => outcome.clone(),
        other => panic!("the run did not end: {other:?}"),
    }
}

#[test]
fn initialize_lists_each_action_as_a_tool_with_its_schema() {
    let (_dir, executor, door) = waiting("serve-tools");
    let mut client = Client::new(executor, door, Limits::default());
    // Nothing is listed before the client initialized.
    client.send(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}));
    assert_eq!(client.read()["error"]["code"], -32002);
    client.initialized();
    client.send(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
    let listed = client.read();
    let tools = listed["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    let registry = whizbang::wiring::registry().unwrap();
    let mut actions: Vec<&str> = registry.descriptors().map(|d| d.name()).collect();
    actions.extend(["waits", "deaf"]);
    assert_eq!(names, actions);
    let tool = |name: &str| tools.iter().find(|t| t["name"] == name).unwrap();
    // `grow`: each argument of its type and bounds, its mask by identity, required.
    let grow = &tool("grow")["inputSchema"];
    assert_eq!(grow["type"], "object");
    assert_eq!(grow["additionalProperties"], false);
    assert_eq!(grow["properties"]["px"]["type"], "number");
    assert_eq!(grow["properties"]["px"]["maximum"], 10_000.0);
    assert_eq!(
        grow["properties"]["mask"]["pattern"],
        "^sha256:[0-9a-f]{64}$"
    );
    let required: Vec<&str> = (grow["required"].as_array().unwrap().iter())
        .map(|name| name.as_str().unwrap())
        .collect();
    assert!(
        required.contains(&"px") && required.contains(&"mask"),
        "{required:?}"
    );
    assert_eq!(grow["properties"]["request_id"]["maxLength"], 128);
    // `check`: the use case, the images with their roles, the brief.
    let check = &tool("check")["inputSchema"];
    assert_eq!(
        check["properties"]["use_case"]["enum"],
        json!(whizbang_squint::use_case::NAMES)
    );
    let role = &check["properties"]["images"]["items"]["properties"]["role"]["enum"];
    assert_eq!(role, &json!(["candidate", "source", "reference"]));
    assert_eq!(check["properties"]["images"]["maxItems"], 10);
    assert_eq!(check["properties"]["brief"]["type"], "string");
    assert!(client.close().is_empty());
}

#[test]
fn initialize_lists_each_recipe_before_the_primitives() {
    let (_dir, executor, door) = waiting("serve-recipes");
    let mut client = Client::new(executor, door, Limits::default());
    client.initialized();
    client.send(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
    let listed = client.read();
    let tools = listed["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    // Every recipe of the registry, and then every other action.
    let registry = whizbang::wiring::registry().unwrap();
    let recipes: Vec<&str> = (registry.descriptors())
        .filter(|d| d.kind() == Kind::Recipe)
        .map(|d| d.name())
        .collect();
    assert_eq!(
        recipes,
        [
            "check",
            "remove_prompt",
            "remove",
            "repair_area",
            "mask_rect",
            "protect",
            "repair",
            "seam",
            "source-fidelity",
            "reference-consistency",
            "reality-checks",
            "brief-compliance",
            "product-fidelity"
        ]
    );
    assert_eq!(names[..recipes.len()], recipes[..]);
    assert!(
        names[recipes.len()..]
            .iter()
            .all(|name| !recipes.contains(name))
    );
    // `remove_prompt` as a tool with its schema: its picture and its protect by
    // identity, its phrase a text, the overrides and the step to stop after.
    let tool = tools.iter().find(|t| t["name"] == "remove_prompt").unwrap();
    assert!(
        tool["description"]
            .as_str()
            .unwrap()
            .starts_with("Removes what a phrase names")
    );
    let schema = &tool["inputSchema"];
    assert_eq!(
        schema["properties"]["image"]["pattern"],
        "^sha256:[0-9a-f]{64}$"
    );
    assert_eq!(
        schema["properties"]["protect"]["pattern"],
        "^sha256:[0-9a-f]{64}$"
    );
    assert_eq!(schema["properties"]["prompt"]["type"], "string");
    assert_eq!(
        schema["properties"]["until"]["enum"],
        json!([
            "segment", "select", "grow", "close", "plug", "subtract", "feather"
        ])
    );
    let required: Vec<&str> = (schema["required"].as_array().unwrap().iter())
        .map(|name| name.as_str().unwrap())
        .collect();
    assert!(
        required.contains(&"image") && required.contains(&"prompt"),
        "{required:?}"
    );
    assert!(!required.contains(&"protect"), "{required:?}");
    assert!(client.close().is_empty());
}

#[test]
fn check_over_stdio_returns_the_verdict_of_each_use_case_under_a_scripted_judge() {
    use crate::anthropic::{
        CHECK_CLASSES, Judged, checklist_of, import_picture, serving, submission_of,
    };
    let judged = Judged::new("serve-check");
    let scratch = judged.world.beside("pictures");
    std::fs::create_dir_all(&scratch).unwrap();
    let root = &judged.world.root;
    let candidate = import_picture(root, &scratch, "candidate", 200);
    let other = import_picture(root, &scratch, "other", 10);
    let brief = "A grey picture with a red band. No text.";
    let cases = [
        ("source-fidelity", Some("source"), 0),
        ("reference-consistency", Some("reference"), 1),
        ("reality-checks", None, 0),
        ("brief-compliance", None, 0),
        ("product-fidelity", Some("reference"), 1),
    ];
    let answers = cases.iter().map(|(use_case, role, references)| {
        let job = whizbang_squint::job::Job::shorthand(
            use_case,
            *role == Some("source"),
            *references,
            Some(brief),
        );
        let ids = checklist_of(&job);
        let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
        submission_of(&ids, "held", "Looked at.")
    });
    let (listener, log) = serving(&judged.pki, answers.collect());
    let origin = format!("https://127.0.0.1:{}", listener.port());
    let over = judged.over_with(
        &judged.address(listener.port()),
        Some(&origin),
        &CHECK_CLASSES,
        "",
    );
    let executor = Arc::new(judged.runner(&over));
    let registry = whizbang::wiring::registry().unwrap();
    let descriptors = registry.descriptors().cloned().collect();
    let door = Arc::new(Actions::new(descriptors, root.clone(), None));
    let mut client = Client::new(executor, door, Limits::default());
    client.initialized();
    for (at, (use_case, role, _)) in cases.iter().enumerate() {
        let mut images = vec![json!({"id": candidate, "role": "candidate"})];
        if let Some(role) = role {
            images.push(json!({"id": other, "role": role}));
        }
        let arguments = json!({"use_case": use_case, "images": images, "brief": brief,
                               "details": "none"});
        let id = at as i64 + 1;
        client.call(id, "check", arguments, None);
        let answer = client.answer(id, &mut Vec::new());
        let (text, error) = result_text(&answer);
        assert!(!error, "{use_case}: {text}");
        let lines: Vec<&str> = text.lines().collect();
        assert!(
            lines[0].starts_with("run ") && lines[0].ends_with(" completed"),
            "{text}"
        );
        assert_eq!(lines[1], "verdict: pass", "{use_case}");
        assert!(lines[2].starts_with("reason: chk:ob:"), "{text}");
    }
    assert!(client.close().is_empty());
    assert_eq!(listener.finish(), cases.len());
    // Each use case's rules reached the judge, in the order of the calls.
    let log = log.lock().unwrap();
    for ((use_case, ..), (_, body)) in cases.iter().zip(log.iter()) {
        let sent: Json = serde_json::from_slice(body).unwrap();
        let rule = match *use_case {
            "source-fidelity" => "SF1",
            "reference-consistency" => "RC1",
            "reality-checks" => "RE1",
            "brief-compliance" => "BC1",
            _ => "PF1",
        };
        assert!(sent["system"].to_string().contains(rule), "{use_case}");
    }
}

#[test]
fn a_use_case_called_with_nothing_set_is_the_control_arm_and_offers_no_resize() {
    // Plan m4, task A1, item 4: over MCP a use case called by a client that sets nothing
    // is the control arm of the gate of details first, whole, by either of its tools:
    // `check` in the shape a caller of a judge asks in, and the use case's recipe by its
    // own name.
    use crate::anthropic::{CHECK_CLASSES, Judged, import_picture, serving, submission};
    use crate::matrix::{argument, runs_of, steps_of};
    let judged = Judged::new("serve-control-arm");
    let scratch = judged.world.beside("pictures");
    std::fs::create_dir_all(&scratch).unwrap();
    let root = &judged.world.root;
    let source = import_picture(root, &scratch, "source", 10);
    let candidate = import_picture(root, &scratch, "candidate", 200);
    // To `check`, the judge calls `align` and then answers; to the recipe, it answers.
    let align = json!({"image": "candidate", "against": "source"});
    let answers = vec![
        crate::check::calling("toolu_a", "align", align),
        submission("held", "Looked at."),
        submission("held", "Looked at."),
    ];
    let (listener, log) = serving(&judged.pki, answers);
    let origin = format!("https://127.0.0.1:{}", listener.port());
    let over = judged.over_with(
        &judged.address(listener.port()),
        Some(&origin),
        &CHECK_CLASSES,
        "",
    );
    let executor = Arc::new(judged.runner(&over));
    let registry = whizbang::wiring::registry().unwrap();
    let descriptors = registry.descriptors().cloned().collect();
    let door = Arc::new(Actions::new(descriptors, root.clone(), None));
    // The schema says what a call with no `details` is.
    let check = (door.tools().into_iter())
        .find(|tool| tool.name == "check")
        .unwrap();
    let details = &check.schema["properties"]["details"];
    assert_eq!(details["default"], "none");
    assert_eq!(details["enum"], json!(["none", "first"]));
    let mut client = Client::new(executor, door, Limits::default());
    client.initialized();
    // `check`, with its use case and its images and nothing else.
    let images = json!([{"id": candidate, "role": "candidate"}, {"id": source, "role": "source"}]);
    let arguments = json!({"use_case": "source-fidelity", "images": images});
    client.call(1, "check", arguments, None);
    let (text, error) = result_text(&client.answer(1, &mut Vec::new()));
    assert!(!error, "{text}");
    assert_eq!(text.lines().nth(1), Some("verdict: pass"), "{text}");
    // The recipe by its own name, with its job and its images and no override.
    let job = whizbang_squint::job::Job::shorthand("source-fidelity", true, 0, None);
    let arguments = json!({"job": job.to_json(), "images": [candidate, source]});
    client.call(2, "source-fidelity", arguments, None);
    let (text, error) = result_text(&client.answer(2, &mut Vec::new()));
    assert!(!error, "{text}");
    assert!(client.close().is_empty());
    assert_eq!(listener.finish(), 3);
    // On the wire: every request offered `view` and `align` and no `resize`, no opening
    // held a detail measured first, and the judge's call of `align` was answered with
    // the fixed sentence and its picture, nothing measured.
    let sent: Vec<Json> = (log.lock().unwrap().iter())
        .map(|(_, body)| serde_json::from_slice(body).unwrap())
        .collect();
    assert_eq!(sent.len(), 3);
    for sent in &sent {
        let offered: Vec<&str> = (sent["tools"].as_array().unwrap().iter())
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert_eq!(offered, ["view", "align", "submit_verdict"]);
        let opening = sent["messages"][0]["content"].as_array().unwrap();
        assert!(
            !opening.iter().any(|part| {
                let text = part["text"].as_str().unwrap_or_default();
                text == whizbang_squint::details::DETAILS_INTRO || text.starts_with("align (")
            }),
            "{opening:?}"
        );
    }
    let answered = &sent[1]["messages"].as_array().unwrap().last().unwrap()["content"][0];
    let content = answered["content"].as_array().unwrap();
    let said: Json = serde_json::from_str(content[0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(
        said["summary"],
        "Result computed; its measurements are withheld."
    );
    assert_eq!(said["measurements"], json!([]));
    assert_eq!(content.iter().filter(|p| p["type"] == "image").count(), 1);
    // In the store: both calls ran the recipe, whose step `details` measured nothing and
    // started no tool, and whose judge's step ran under the control arm's settings. The
    // door's call states them; the recipe's own call leaves them to the recipe.
    let recipes = runs_of(root, "source-fidelity");
    assert_eq!(recipes.len(), 2);
    let stated: Vec<bool> = (recipes.iter())
        .map(|(_, record)| argument(record, "set").is_some())
        .collect();
    assert_eq!(stated.iter().filter(|stated| **stated).count(), 1);
    let details = runs_of(root, "details");
    assert_eq!(details.len(), 2);
    for (_, record) in &details {
        assert_eq!(argument(record, "details").as_deref(), Some("none"));
        assert_eq!(steps_of(record), [] as [&str; 0]);
    }
    let checks = runs_of(root, "check");
    assert_eq!(checks.len(), 2);
    for (_, record) in &checks {
        assert_eq!(argument(record, "tools").as_deref(), Some("view,align"));
        assert_eq!(
            argument(record, "projection").as_deref(),
            Some("visual_only")
        );
    }
}

#[test]
fn nothing_but_the_protocol_is_written_to_stdout() {
    let (_dir, executor, door) = waiting("serve-protocol");
    let mut client = Client::new(executor, door, Limits::default());
    client.initialized();
    // Each of these is answered with one message and nothing else; `Client::read` and
    // `Client::close` hold every line to the protocol.
    client.line("not json");
    client.line(&"[".repeat(40));
    client.line(&format!(
        "{{\"pad\":\"{}\"}}",
        "x".repeat(whizbang_serve::frame::MESSAGE_LIMIT)
    ));
    client.line("{\"jsonrpc\":\"1.0\",\"id\":5,\"method\":\"ping\"}");
    client.send(&json!({"jsonrpc": "2.0", "id": 6, "method": "no/such"}));
    client.send(&json!({"jsonrpc": "2.0", "id": {"an": "object"}, "method": "ping"}));
    client.send(&json!({"jsonrpc": "2.0", "method": "notifications/whatever"}));
    client.call(7, "grow", json!({"mask": "not an identity", "px": 2}), None);
    client.call(8, "nothing-of-that-name", json!({}), None);
    client.send(&json!({"jsonrpc": "2.0", "id": 9, "method": "ping"}));
    let codes: Vec<Json> = (0..9).map(|_| client.read()).collect();
    let errors: Vec<&Json> = codes.iter().map(|m| &m["error"]["code"]).collect();
    assert_eq!(
        errors[..6],
        [-32700, -32600, -32600, -32600, -32601, -32600]
    );
    let (text, error) = result_text(&codes[6]);
    assert!(error);
    assert_eq!(
        text,
        "mask is no artifact identity: an identity is sha256: and 64 lower-case hex digits"
    );
    assert_eq!(codes[7]["error"]["code"], -32602);
    assert_eq!(codes[8], json!({"jsonrpc": "2.0", "id": 9, "result": {}}));
    assert!(client.close().is_empty());
}

#[test]
fn progress_reaches_the_client_as_notifications() {
    let (_dir, executor, door) = waiting("serve-progress");
    let mut client = Client::new(executor.clone(), door, Limits::default());
    client.initialized();
    client.call(1, "waits", json!({}), Some("tok"));
    let mut notes = Vec::new();
    until_said(&mut client, &whizbang_serve::escaped(SAID[1]), &mut notes);
    // Each event reached the client while the run was still going, in order, under the
    // token, its control characters escaped.
    let run = started(&notes);
    assert!(matches!(
        executor.status(&run).unwrap().state(),
        RunState::NotEnded { here: true }
    ));
    let said: Vec<&str> = (notes.iter())
        .map(|note| note["params"]["message"].as_str().unwrap())
        .collect();
    assert_eq!(
        said[1..],
        [
            SAID[0],
            "a \\u{1b}[31mred\\u{1b}[0m word\\nand a second line"
        ]
    );
    let progress: Vec<i64> = notes
        .iter()
        .map(|n| n["params"]["progress"].as_i64().unwrap())
        .collect();
    assert_eq!(progress, [1, 2, 3]);
    assert!(notes.iter().all(|n| n["params"]["progressToken"] == "tok"));
    // The end of the input cancels the run, and the server waits for it.
    assert!(client.close().is_empty());
    assert_eq!(ended(&executor, &run), Outcome::Cancelled);
}

#[test]
fn a_cancelled_call_ends_its_run_as_cancelled() {
    let (_dir, executor, door) = waiting("serve-cancel");
    let mut client = Client::new(executor.clone(), door, Limits::default());
    client.initialized();
    client.call(1, "waits", json!({}), Some("tok"));
    let mut notes = Vec::new();
    until_said(&mut client, SAID[0], &mut notes);
    let run = started(&notes);
    client.send(
        &json!({"jsonrpc": "2.0", "method": "notifications/cancelled",
                        "params": {"requestId": 1, "reason": "no longer wanted"}}),
    );
    assert_eq!(ended(&executor, &run), Outcome::Cancelled);
    // The cancelled call is never answered: the next message is the ping's answer.
    client.send(&json!({"jsonrpc": "2.0", "id": 2, "method": "ping"}));
    let mut after = Vec::new();
    let pong = client.answer(2, &mut after);
    assert_eq!(pong["result"], json!({}));
    assert!(
        after.iter().all(|note| note.get("id").is_none()),
        "{after:?}"
    );
    assert!(client.close().iter().all(|m| m.get("id").is_none()));
}

#[test]
fn a_run_waiting_past_its_stated_time_gives_up() {
    let (_dir, executor, door) = waiting("serve-gives-up");
    let limits = Limits {
        wait: Duration::from_millis(300),
        ..Limits::default()
    };
    let mut client = Client::new(executor.clone(), door, limits);
    client.initialized();
    client.call(1, "waits", json!({}), Some("tok"));
    let mut notes = Vec::new();
    let answer = client.answer(1, &mut notes);
    let run = started(&notes);
    let (text, error) = result_text(&answer);
    assert!(error);
    let said = format!(
        "gave up on run {} after its stated time of 0.3 s, and cancelled it\nrun {} ",
        run.as_str(),
        run.as_str()
    );
    assert!(text.starts_with(&said), "{text}");
    // The run had its deadline at the stated time too, and ended.
    assert!(matches!(
        ended(&executor, &run),
        Outcome::Cancelled | Outcome::Error { .. }
    ));
    assert!(client.close().is_empty());
}

#[test]
fn a_run_its_deadline_ended_before_the_server_looked_is_answered_as_given_up() {
    // The race behind a flake of the test above (the review of task M1): the run's own
    // deadline, at the stated time, ends it before the server looks again. Looking
    // only every 1.5 s makes that the order every time.
    let (_dir, executor, door) = waiting("serve-gives-up-late");
    let limits = Limits {
        wait: Duration::from_millis(200),
        look: Duration::from_millis(1500),
        ..Limits::default()
    };
    let mut client = Client::new(executor.clone(), door, limits);
    client.initialized();
    client.call(1, "waits", json!({}), Some("tok"));
    let mut notes = Vec::new();
    let answer = client.answer(1, &mut notes);
    let run = started(&notes);
    let (text, error) = result_text(&answer);
    assert!(error);
    let said = format!(
        "gave up on run {} after its stated time of 0.2 s",
        run.as_str()
    );
    assert!(text.starts_with(&said), "{text}");
    assert!(client.close().is_empty());
}

#[test]
fn end_of_input_cancels_waits_and_stops_the_worker() {
    use crate::pack::{Local, one};
    use whizbang_backends::pack::queue::Status;
    let local = Local::new("serve-end");
    let over = local.over(&one("fake-wait", "call_limit_s = 20\n"));
    let executor = Arc::new(local.runner(&over));
    let descriptors = local.registry().descriptors().cloned().collect();
    let worker: Arc<whizbang::pack::OneWorker> = (*local.worker).clone();
    let door = Arc::new(Actions::new(descriptors, local.root.clone(), Some(worker)));
    let mut client = Client::new(executor.clone(), door, Limits::default());
    client.initialized();
    client.call(1, "finds", json!({}), Some("tok"));
    let mut notes = vec![client.read()];
    let run = started(&notes);
    // The model's call is in the worker, and waits for a cancel.
    let written = |status: &Status| {
        status
            .calling
            .as_ref()
            .is_some_and(|calling| calling.kind == "segment" && calling.written)
    };
    assert!(local.worker.wait_for(Duration::from_secs(20), &written));
    // The input ends: the run is cancelled and waited for, and then the worker stopped.
    notes.extend(client.close());
    assert!(notes.iter().all(|m| m.get("id").is_none()), "{notes:?}");
    assert_eq!(ended(&executor, &run), Outcome::Cancelled);
    assert!(whizbang_testkit::worker::pack_children().is_empty());
}

#[test]
fn a_run_given_up_after_its_grace_no_longer_counts_toward_the_limit_of_calls() {
    // The bound as it is (ADR 0031, "Threads"; the review of task M1): a run whose action
    // ignores its cancel is answered and dropped from the server's list once the grace
    // after its stated time has passed, though it may still be running. The server's
    // limit of calls then lets another start beside it; only the executor's own bound
    // on started runs holds them both.
    let (_dir, executor, door) = waiting("serve-grace");
    let limits = Limits {
        wait: Duration::from_millis(100),
        grace: Duration::from_millis(100),
        calls: 1,
        ..Limits::default()
    };
    let mut client = Client::new(executor.clone(), door, limits);
    client.initialized();
    client.call(1, "deaf", json!({}), Some("one"));
    let mut notes = Vec::new();
    let (text, error) = result_text(&client.answer(1, &mut notes));
    assert!(
        error && text.ends_with("it was cancelled and has not ended"),
        "{text}"
    );
    let first = started(&notes);
    assert!(matches!(
        executor.status(&first).unwrap().state(),
        RunState::NotEnded { here: true }
    ));
    // A second call starts while the first still runs: the limit of one is not met.
    client.call(2, "deaf", json!({}), Some("two"));
    let mut second = Vec::new();
    let answer = client.answer(2, &mut second);
    let (text, _) = result_text(&answer);
    assert!(!text.starts_with("the server is busy"), "{text}");
    let second = started(&second);
    for run in [&first, &second] {
        executor.wait(run).unwrap();
    }
    drop(client.close());
}

const LINGERS: &str = r#"import subprocess, time
def run(context):
    child = subprocess.Popen(['/bin/sleep', '30'])
    with open(context.arguments['phrase'] + '.new', 'w') as noted:
        noted.write(str(child.pid))
    import os
    os.rename(context.arguments['phrase'] + '.new', context.arguments['phrase'])
    time.sleep(30)
    return {}
"#;

#[test]
fn end_of_input_ends_the_tool_workers_group_with_a_child_holding_its_pipes() {
    use crate::tool_file::{BOUNDS, Tools, tool_text};
    let tools = Tools::new(
        "serve-tool-group",
        &[("lingers.py", tool_text("lingers", BOUNDS, LINGERS))],
    );
    let over = tools.over(None);
    let executor = Arc::new(tools.runner(&over));
    let registry = whizbang::wiring::registry_over(&over, false).unwrap();
    let descriptors = registry.descriptors().cloned().collect();
    let worker: Arc<whizbang::pack::OneWorker> = (*tools.worker).clone();
    let door = Arc::new(Actions::new(descriptors, tools.root.clone(), Some(worker)));
    let mut client = Client::new(executor.clone(), door, Limits::default());
    client.initialized();
    // The tool starts a process that holds the worker's input and error output, notes
    // its id where the test says, and waits, deaf to everything.
    let noted = tools.dir.path().join("child");
    let arguments = json!({"source": tools.image, "candidate": tools.image,
        "phrase": noted.to_str().unwrap()});
    client.call(1, "lingers", arguments, Some("tok"));
    let mut notes = vec![client.read()];
    let run = started(&notes);
    let mut child = None;
    for _ in 0..2_000 {
        if let Ok(text) = std::fs::read_to_string(&noted) {
            child = text.trim().parse::<u32>().ok();
            break;
        }
        std::thread::park_timeout(Duration::from_millis(5));
    }
    let child = child.expect("the tool noted its child");
    // The input ends: the run is cancelled, and the tool worker's group ended, the
    // child with it, and waited for.
    notes.extend(client.close());
    assert_eq!(ended(&executor, &run), Outcome::Cancelled, "{notes:?}");
    let lives = whizbang_testkit::worker::processes()
        .iter()
        .any(|listed| listed.pid == child && !listed.state.starts_with('Z'));
    assert!(!lives, "the tool's child {child} outlived the server");
    assert_eq!(tools.worker.tool_pid(), None);
    assert!(whizbang_testkit::worker::pack_children().is_empty());
}

#[test]
fn a_client_names_a_picture_by_its_identity_in_the_store_and_never_by_a_path_or_its_bytes() {
    // Plan m3, task O5 (pictures from callers): over MCP a client passes a picture, or
    // any artifact, as its identity in the store, `sha256:` and 64 hex digits, and
    // nothing else. No tool takes a path, a file's bytes or base64; a picture comes
    // into the store by `whizbang store import`, on the command line, alone.
    let (_dir, _executor, door) = waiting("serve-pictures");
    let registry = whizbang::wiring::registry().unwrap();
    let identity = json!("^sha256:[0-9a-f]{64}$");
    for tool in door.tools() {
        let properties = tool.schema["properties"].as_object().unwrap();
        assert_eq!(tool.schema["additionalProperties"], false, "{}", tool.name);
        // `check` is offered in a shape of its own, held below.
        let found = registry.descriptors().find(|d| d.name() == tool.name);
        let Some(descriptor) = found.filter(|_| tool.name != "check") else {
            continue;
        };
        for port in descriptor.inputs() {
            let schema = &properties[port.name()];
            let pattern = schema.get("pattern").or(schema["items"].get("pattern"));
            assert_eq!(pattern, Some(&identity), "{}: {}", tool.name, port.name());
        }
        for argument in descriptor.arguments() {
            let name = argument.name();
            // (A `path` of `line` and `stroke` is points on the picture, not a file's.)
            let words = ["file", "dir", "folder", "base64", "bytes", "url"];
            assert!(
                !name.split('_').any(|part| words.contains(&part)),
                "{}: {name}",
                tool.name
            );
        }
    }
    let check = door
        .tools()
        .into_iter()
        .find(|t| t.name == "check")
        .unwrap();
    let image = &check.schema["properties"]["images"]["items"];
    let keys: Vec<&str> = image["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["id", "label", "role"]);
    assert_eq!(image["additionalProperties"], false);
    assert_eq!(image["properties"]["id"]["pattern"], identity);
    // A path, a data address or base64 where an identity belongs is refused before a
    // run, and so is a property no tool has.
    let call = |name: &str, arguments: Json| door.call(name, arguments.as_object().unwrap());
    let digest = format!("sha256:{}", "0".repeat(64));
    for after in [
        "/etc/hosts",
        "data:image/png;base64,iVBORw0KGgo=",
        "aVZCT1J3MEtHZ289",
    ] {
        let refused = call("measure", json!({"after": after, "mask": digest})).unwrap_err();
        assert!(refused.contains("no artifact identity"), "{refused}");
        assert!(!refused.contains(after), "{refused}");
    }
    let refused = call(
        "measure",
        json!({"after": digest, "mask": digest, "picture": "aVZCT1J3MEtHZ289"}),
    )
    .unwrap_err();
    assert!(refused.contains("does not take"), "{refused}");
    for picture in [
        json!({"path": "/etc/hosts", "role": "candidate"}),
        json!({"id": digest, "role": "candidate", "base64": "aVZCT1J3MEtHZ289"}),
        json!({"id": "/etc/hosts", "role": "candidate"}),
    ] {
        let refused = call(
            "check",
            json!({"use_case": "reality-checks", "images": [picture]}),
        )
        .unwrap_err();
        assert!(refused.starts_with("images 1:"), "{refused}");
        assert!(!refused.contains("/etc/hosts"), "{refused}");
    }
}

#[test]
fn a_request_id_reserved_for_a_labs_cells_is_refused_and_nothing_runs() {
    let (_dir, executor, door) = waiting("serve-reserved");
    let mut client = Client::new(executor, door, Limits::default());
    client.initialized();
    let id = "cell.lab-0123456789ab-0123456789abcdef.0123abcd.1";
    client.call(1, "deaf", json!({"request_id": id}), None);
    let mut notes = Vec::new();
    let answer = client.answer(1, &mut notes);
    let (text, error) = result_text(&answer);
    assert!(error, "{answer}");
    assert!(text.contains("reserved for the cells of a lab"), "{text}");
    assert!(notes.is_empty(), "{notes:?}");
    drop(client.close());
}

// --- a planned call (plan m4, task K1, item 1; ADR 0004, "A call's plan") -----------------------

/// What `plans` did, in order: each plan it stated and each run it made.
pub type Events = Arc<std::sync::Mutex<Vec<String>>>;

/// How many attempts the plan of `plans` allows at most: what a test changes where a
/// config's cap would change, between two preparations of one call.
pub static ALLOWS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(2);

/// `plans`: a fake action that plans. Asked for `attempts`, its plan allows two of them
/// at most, unchecked; with `plain` it plans nothing. Its run notes that it ran.
struct Plans(ActionDescriptor, Events);

impl Action for Plans {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }

    fn plan(&self, planning: &Planning<'_>) -> Option<Plan> {
        if planning.argument("plain") == Some(&Value::Boolean(true)) {
            return None;
        }
        let Some(Value::Integer(asked)) = planning.argument("attempts") else {
            return None;
        };
        let asked = *asked as u32;
        let most = ALLOWS.load(std::sync::atomic::Ordering::SeqCst);
        let plan = Plan::new().attempts(asked, asked.min(most), false);
        Some(plan.because("a test's plan"))
    }

    fn run(&self, call: &PreparedCall, _: &Context) -> Result<Produced, Failure> {
        let handed = call.plan().map(Plan::line);
        self.1.lock().unwrap().push(format!("ran under {handed:?}"));
        Ok(Produced::nothing())
    }
}

/// A store, the binary's runner over it with `plans` beside the build's actions, the
/// door over the same actions, and what `plans` did.
pub fn planning(label: &str) -> (Scratch, Arc<Engine>, Arc<Actions>, Events) {
    let dir = Scratch::new(label);
    let root = dir.path().join("store");
    let store = Arc::new(Store::open(&root, Arc::new(RealFs)).unwrap());
    let events = Events::default();
    let draft = ActionDescriptor::draft("plans", "1.0.0", Kind::Loop, "A test action that plans.")
        .argument(Argument::with_default(
            "attempts",
            "How many attempts, at most.",
            ValueType::integer(Some(1), Some(3)),
            Value::Integer(3),
        ))
        .argument(Argument::optional(
            "plain",
            "Plans nothing.",
            ValueType::Boolean,
        ))
        .plans();
    let mut registry: Registry = whizbang::wiring::registry().unwrap();
    let plans = Plans(draft.build().unwrap(), events.clone());
    registry.register(Arc::new(plans)).unwrap();
    let descriptors = registry.descriptors().cloned().collect();
    let engine = crate::runner(registry, store);
    let door = Arc::new(Actions::new(descriptors, root, None));
    (dir, Arc::new(engine), door, events)
}

#[test]
fn a_planned_call_answers_with_its_plan() {
    let (_dir, executor, door, events) = planning("serve-plan");
    let mut client = Client::new(executor, door, Limits::default());
    client.initialized();
    let mut notes = Vec::new();
    client.call(1, "plans", json!({"attempts": 3}), None);
    let (text, error) = result_text(&client.answer(1, &mut notes));
    assert!(!error, "{text}");
    // The plan the run was admitted under, then the run.
    let mut lines = text.lines();
    assert_eq!(
        lines.next(),
        Some("plan: 2 of 3 attempts asked, unchecked; a test's plan")
    );
    let run = lines.next().unwrap();
    assert!(
        run.starts_with("run ") && run.ends_with(" completed"),
        "{text}"
    );
    // And it is the plan the action was handed.
    assert_eq!(
        *events.lock().unwrap(),
        ["ran under Some(\"plan: 2 of 3 attempts asked, unchecked; a test's plan\")"]
    );
    // A call the action plans nothing for answers with no plan.
    client.call(2, "plans", json!({"plain": true}), None);
    let (text, error) = result_text(&client.answer(2, &mut notes));
    assert!(
        !error && text.starts_with("run ") && !text.contains("plan:"),
        "{text}"
    );
    client.close();
}

#[test]
fn check_over_stdio_answers_with_the_counts_of_its_findings() {
    use crate::anthropic::{
        CHECK_CLASSES, Judged, checklist_of, import_picture, serving, submission_of,
    };
    // Plan m4, task K2, item 4: the server's answer for `check` carries what the run's
    // record keeps of its findings, after the verdict and its reasons, as the command
    // prints them.
    let judged = Judged::new("serve-findings");
    let scratch = judged.world.beside("pictures");
    std::fs::create_dir_all(&scratch).unwrap();
    let root = &judged.world.root;
    let candidate = import_picture(root, &scratch, "candidate", 200);
    let source = import_picture(root, &scratch, "other", 10);
    let job = whizbang_squint::job::Job::shorthand("source-fidelity", true, 0, None);
    let ids = checklist_of(&job);
    let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
    let answers = vec![submission_of(&ids, "violation", "Looked at.")];
    let (listener, _) = serving(&judged.pki, answers);
    let origin = format!("https://127.0.0.1:{}", listener.port());
    let over = judged.over_with(
        &judged.address(listener.port()),
        Some(&origin),
        &CHECK_CLASSES,
        "",
    );
    let executor = Arc::new(judged.runner(&over));
    let registry = whizbang::wiring::registry().unwrap();
    let descriptors = registry.descriptors().cloned().collect();
    let door = Arc::new(Actions::new(descriptors, root.clone(), None));
    let mut client = Client::new(executor, door, Limits::default());
    client.initialized();
    let images = json!([{"id": candidate, "role": "candidate"}, {"id": source, "role": "source"}]);
    let arguments = json!({"use_case": "source-fidelity", "images": images, "details": "none"});
    client.call(1, "check", arguments, None);
    let answer = client.answer(1, &mut Vec::new());
    let (text, error) = result_text(&answer);
    assert!(!error, "{text}");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[1], "verdict: fail", "{text}");
    assert_eq!(
        lines[lines.len() - 3..],
        [
            "findings: complete",
            "finding: SF1 violation confident: 1",
            "finding: SF2 violation confident: 1"
        ],
        "{text}"
    );
    assert!(client.close().is_empty());
    assert_eq!(listener.finish(), 1);
}

#[test]
fn the_loop_is_offered_as_a_tool_and_answers_with_its_plan_its_verdict_and_whether_it_was_checked()
{
    use crate::checked::{Looped, PHRASE, Setup, passes};
    // Plan m4, task L1, item 10: the loop over stdio, in a store on the real disk,
    // which the server reads, under a judge that passes the one attempt asked.
    let setup = Setup::answering(vec![passes()]).on_disk();
    let world = Looped::new("serve-loop", &setup);
    let engine = world.engine();
    let registry = whizbang::wiring::registry_over(&world.over, false).unwrap();
    let descriptors: Vec<_> = registry.descriptors().cloned().collect();
    let door = Arc::new(Actions::new(descriptors, world.root.clone(), None));
    // It is offered after the recipes and before the primitives, as a loop.
    let offered: Vec<String> = (whizbang_serve::Door::tools(door.as_ref()).iter())
        .map(|tool| tool.name.clone())
        .collect();
    let at = offered
        .iter()
        .position(|name| name == "remove_checked")
        .unwrap();
    assert_eq!(offered[at - 1], "product-fidelity");
    assert_eq!(offered[at + 1], "align");
    let mut client = Client::new(Arc::new(engine), door, Limits::default());
    client.initialized();
    let mut notes = Vec::new();
    client.call(
        1,
        "remove_checked",
        json!({"image": world.image, "prompt": PHRASE, "attempts": 1}),
        None,
    );
    let (text, error) = result_text(&client.answer(1, &mut notes));
    assert!(!error, "{text}");
    let lines: Vec<&str> = text.lines().collect();
    // Its plan, its run, the short result, and the kept attempt's verdict.
    assert_eq!(
        lines[0],
        "plan: 1 of 1 attempts asked, checked; fill 1; judge 16"
    );
    assert!(
        lines[1].starts_with("run ") && lines[1].ends_with(" completed"),
        "{text}"
    );
    assert_eq!(
        lines[2..6],
        [
            "attempts: 1 made, attempt 1 kept",
            "ended: attempt 1 passed",
            "checked: yes",
            "verdict: pass",
        ]
    );
    assert!(
        text.contains("\nimage sha256:") && text.contains("\nmask sha256:"),
        "{text}"
    );
    // Unchecked, it says so, and why.
    client.call(
        2,
        "remove_checked",
        json!({"image": world.image, "prompt": PHRASE, "check": "off"}),
        None,
    );
    let (text, error) = result_text(&client.answer(2, &mut notes));
    assert!(!error, "{text}");
    assert!(
        text.starts_with("plan: 1 of 3 attempts asked, unchecked;"),
        "{text}"
    );
    assert!(text.contains("\nchecked: no: the check is off\n"), "{text}");
    assert!(!text.contains("verdict:"), "{text}");
    client.close();
}
