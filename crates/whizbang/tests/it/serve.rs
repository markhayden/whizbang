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
    Action, ActionDescriptor, Context, Engine, Executor, Failure, Kind, Outcome, PreparedCall,
    Produced, Registry, RunId, RunState,
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
            "protect"
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
