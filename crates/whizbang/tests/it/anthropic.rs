//! `anthropic::…`: the judge of a service through the binary's wiring (plan 2A, task
//! S1): a config that names the kind `service` and its key, its env file, the
//! executor `wiring::runner` builds with the network client as its sender, and a
//! server on loopback with a certificate of a root made in the test, which the
//! sender is told to trust (`Over::trusting`, under the feature `fakes`). The kind is
//! made at the server's address (`ServiceKind::at`); the shipped kind's address is
//! never asked. Every key here is made up at run time.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use whizbang::wiring::{self, Over};
use whizbang_backends::anthropic::{KEY_HEADER, ServiceKind};
use whizbang_backends::net::testing::Pki;
use whizbang_core::capability::{Controls, JudgeRequest, Message, Part, Role};
use whizbang_core::config::Kinds;
use whizbang_core::contract::{
    Action, ActionDescriptor, Capability, Context, Engine, Failure, Kind, PreparedCall, Produced,
    Registry, Requirement,
};
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;
use whizbang_testkit::loopback::Listener;

use crate::wired::{World, whizbang};

/// The variable the test's config names for the key.
pub const KEY_VARIABLE: &str = "TEST_SERVICE_KEY";

/// A made-up key, long enough to be a secret, put together here so that no scan of
/// this file finds one.
pub fn made_up_key() -> String {
    ["made", "up", "service", "key", "0123456789"].join("-")
}

/// `judges`: an action that needs a judge and asks it once, a request of words. It
/// fails, as an error of the kind `judge`, with the fault's sentence.
struct Judges(ActionDescriptor);

impl Action for Judges {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }

    fn run(&self, _: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let judge = context.capabilities().judge().expect("a judge is handed");
        let message = Message::new(Role::User, vec![Part::Text("Look.".to_owned())]).unwrap();
        let request = JudgeRequest::new(
            "Judge.",
            vec![message],
            Vec::new(),
            Controls::new(64).unwrap(),
        );
        match judge.send(&request.unwrap()) {
            Ok(_) => Ok(Produced::nothing()),
            Err(fault) => Err(Failure::error("judge", &fault.to_string())),
        }
    }
}

fn judges() -> Arc<dyn Action> {
    let draft = ActionDescriptor::draft("judges", "1.0.0", Kind::Primitive, "A test action.")
        .needs(Requirement::new(Capability::Judge));
    Arc::new(Judges(draft.build().unwrap()))
}

/// What a server on loopback saw of each request: its head and its body.
pub type Log = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

/// A server of TLS on loopback that answers each of its connections with the next of
/// `answers`, and keeps what each request was.
pub fn serving(pki: &Arc<Pki>, answers: Vec<Vec<u8>>) -> (Listener, Log) {
    let log: Log = Arc::default();
    let (kept, pki) = (log.clone(), pki.clone());
    let listener = Listener::serve(answers.len(), move |number, stream| {
        let mut tls = pki.accept(stream);
        let mut head = Vec::new();
        let mut byte = [0];
        while !head.ends_with(b"\r\n\r\n") && head.len() < 64 * 1024 {
            match tls.read(&mut byte) {
                Ok(1) => head.push(byte[0]),
                _ => break,
            }
        }
        let head = String::from_utf8_lossy(&head).into_owned();
        let length = head
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())?
            })
            .unwrap_or(0);
        let mut body = vec![0; length];
        let read = tls.read_exact(&mut body);
        kept.lock().unwrap().push((head, body));
        if read.is_ok() && tls.write_all(&answers[number]).is_ok() {
            tls.conn.send_close_notify();
            let _ = tls.flush();
        }
    });
    (listener, log)
}

/// A provider of the kind `service`, as a test's config names it: its name, the model
/// a test names, and the path of its address, which the test's server is at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Provider {
    pub name: &'static str,
    pub model: &'static str,
    pub path: &'static str,
}

pub const ANTHROPIC: Provider = Provider {
    name: "anthropic",
    model: "claude-haiku-5-5",
    path: "/v1/messages",
};

pub const GOOGLE: Provider = Provider {
    name: "google",
    model: "gemini-3.8-flash",
    path: "/v1beta/models/gemini-3.8-flash:generateContent",
};

pub const OPENAI: Provider = Provider {
    name: "openai",
    model: "gpt-6.1-terra",
    path: "/v1/responses",
};

/// A world whose config names one judge of the kind `service`, at `address`, with the
/// key in its env file, and approves `approved` for prompts when it names one.
pub struct Judged {
    pub world: World,
    pub key: String,
    pub pki: Arc<Pki>,
    pub provider: Provider,
}

impl Judged {
    pub fn new(label: &str) -> Judged {
        Judged::of(label, ANTHROPIC)
    }

    /// The same, of this provider.
    pub fn of(label: &str, provider: Provider) -> Judged {
        let world = World::new(label);
        let reset = whizbang(&["spend", "reset", "--allowance", "20"], &world.root);
        assert_eq!(reset.code, Some(0), "{}", reset.err);
        Judged {
            world,
            key: made_up_key(),
            pki: Arc::new(Pki::loopback()),
            provider,
        }
    }

    /// The address of the provider at a port of loopback.
    pub fn address(&self, port: u16) -> String {
        format!("https://127.0.0.1:{port}{}", self.provider.path)
    }

    /// What the runner is built over: the config and the env file, written; the kind
    /// at `address`; and the sender trusting the test's root.
    pub fn over(&self, address: &str, approved: Option<&str>) -> Over {
        self.over_with(address, approved, &["prompts"], "")
    }

    /// The same, approving `approved` for these classes, with `more` written after the
    /// judge's table: its settings, or the table `spend`.
    pub fn over_with(
        &self,
        address: &str,
        approved: Option<&str>,
        classes: &[&str],
        more: &str,
    ) -> Over {
        let Provider { name, model, .. } = self.provider;
        let mut config = format!(
            "[backends.judge]\nkind = \"service\"\nprovider = \"{name}\"\n\
             model = \"{model}\"\nkey_env = \"{KEY_VARIABLE}\"\n\n{more}"
        );
        if let Some(origin) = approved {
            let classes: Vec<String> = classes.iter().map(|class| format!("\"{class}\"")).collect();
            config.push_str(&format!(
                "\n[egress.recipients.service]\norigin = \"{origin}\"\nclasses = [{}]\n",
                classes.join(", ")
            ));
        }
        std::fs::write(self.world.config_path(), config).unwrap();
        std::fs::write(
            self.world.env_path(),
            format!("{KEY_VARIABLE}={}\n", self.key),
        )
        .unwrap();
        let mut kinds = Kinds::new();
        kinds.register(Arc::new(ServiceKind::at(address))).unwrap();
        Over::nothing()
            .config(Some(&self.world.config_path()))
            .env_file(Some(&self.world.env_path()))
            .kinds(kinds)
            .trusting(self.pki.clone())
    }

    /// The binary's runner over the world's store, with `judges` registered.
    pub fn runner(&self, over: &Over) -> Engine {
        let mut registry: Registry = wiring::registry().unwrap();
        registry.register(judges()).unwrap();
        let store = Arc::new(Store::open(&self.world.root, Arc::new(RealFs)).unwrap());
        wiring::runner(registry, store, over).expect("the executor builds")
    }

    /// Whether any file of the store holds the key.
    pub fn store_holds_the_key(&self) -> bool {
        fn walk(dir: &Path, key: &[u8]) -> bool {
            std::fs::read_dir(dir).unwrap().any(|entry| {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, key)
                } else {
                    let bytes = std::fs::read(&path).unwrap();
                    bytes.windows(key.len()).any(|at| at == key)
                }
            })
        }
        walk(&self.world.root, self.key.as_bytes())
    }
}

/// A call of `judges` under a judge of this provider at a server on loopback whose
/// origin the config approves: what the server saw of the one request, its head and its
/// body, once the store is checked to hold no byte of the key.
pub fn asked_once(provider: Provider, label: &str) -> (Judged, String, String) {
    use whizbang_core::contract::{Call, Executor};
    let judged = Judged::of(label, provider);
    let (listener, log) = serving(&judged.pki, vec![said_by(provider, "Seen.", (9, 0, 2))]);
    let origin = format!("https://127.0.0.1:{}", listener.port());
    let over = judged.over(&judged.address(listener.port()), Some(&origin));
    let executor = judged.runner(&over);
    let ran = executor.submit(&Call::new("judges")).unwrap();
    assert_eq!(ran.outcome().name(), "completed");
    drop(executor);
    assert_eq!(listener.finish(), 1);
    let (head, body) = log.lock().unwrap()[0].clone();
    let body = String::from_utf8(body).unwrap();
    // The key is in the head once, and in no address and no body; the store holds no
    // byte of it.
    assert_eq!(head.matches(&judged.key).count(), 1, "{head}");
    assert!(
        !head.lines().next().unwrap().contains(&judged.key),
        "{head}"
    );
    assert!(!body.contains(&judged.key));
    assert!(!judged.store_holds_the_key());
    (judged, head, body)
}

#[test]
fn the_key_goes_only_to_the_approved_origin_in_its_header() {
    let (judged, head, _) = asked_once(ANTHROPIC, "anthropic-key");
    // In its one header.
    let line = format!("{KEY_HEADER}: {}\r\n", judged.key);
    assert!(head.contains(&line), "{head}");
    assert!(head.starts_with("POST /v1/messages HTTP/1.1\r\n"), "{head}");
    assert!(head.contains("anthropic-version: 2023-06-01\r\n"), "{head}");
}

/// An answer of 200 with a turn that says `text`, stops for `stop`, and reports this
/// usage, input and output, or none.
pub fn turn_with(text: &str, stop: &str, usage: Option<(u64, u64)>) -> Vec<u8> {
    let mut body = serde_json::json!({
        "id": "msg_1", "type": "message", "role": "assistant", "model": "claude-haiku-5-5",
        "content": [{ "type": "text", "text": text }], "stop_reason": stop
    });
    if let Some((input, output)) = usage {
        body["usage"] = serde_json::json!({ "input_tokens": input, "output_tokens": output });
    }
    let body = body.to_string();
    format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

/// A picture of `width` by `height` written as a PNG and brought into the store at
/// `root` by the real binary's `store import --picture`: its identity.
pub fn import_picture(root: &Path, scratch: &Path, name: &str, shade: u8) -> String {
    let (width, height) = (64u32, 48u32);
    let rgb: Vec<u8> = (0..width * height)
        .flat_map(|at| [shade, (at % 251) as u8, (at / 64) as u8])
        .collect();
    let file = scratch.join(format!("{name}.png"));
    image::RgbImage::from_raw(width, height, rgb)
        .unwrap()
        .save_with_format(&file, image::ImageFormat::Png)
        .unwrap();
    let imported = whizbang(
        &["store", "import", "--picture", file.to_str().unwrap()],
        root,
    );
    assert_eq!(imported.code, Some(0), "{}", imported.err);
    let last = imported.out.lines().last().unwrap();
    let id = last
        .strip_prefix("imported image ")
        .or_else(|| last.strip_prefix("already there: image "));
    id.unwrap().to_owned()
}

/// A run's record, as `runs show --json` gives it.
pub fn record_of(root: &Path, run: &str) -> serde_json::Value {
    let shown = whizbang(&["runs", "show", run, "--json"], root);
    assert_eq!(shown.code, Some(0), "{}", shown.err);
    serde_json::from_str(&shown.out).unwrap()
}

/// A call of `judges`.
pub fn judges_call() -> whizbang_core::contract::Call {
    whizbang_core::contract::Call::new("judges")
}

/// The input of a call of `submit_verdict` that gives each checklist item of
/// source-fidelity this disposition, confidently, and a reason that says `said`.
pub fn verdict_input(disposition: &str, said: &str) -> serde_json::Value {
    verdict_input_of(
        &["chk:ob:SF1@candidate", "chk:ob:SF2@candidate"],
        disposition,
        said,
    )
}

/// The same, for the checklist of these items.
pub fn verdict_input_of(items: &[&str], disposition: &str, said: &str) -> serde_json::Value {
    let item = |id: &str| {
        serde_json::json!({
            "id": id, "disposition": disposition, "confidence": "confident",
            "reason": said, "allowed_by": null, "evidence": [],
            "location": if disposition == "violation" {
                serde_json::json!({"kind": "whole_image", "frame": "raw:candidate", "bbox": null,
                                   "polygon": null, "yaw": null, "pitch": null, "fov": null})
            } else { serde_json::Value::Null }
        })
    };
    serde_json::json!({ "schema_version": 1, "findings": [],
                        "items": items.iter().map(|id| item(id)).collect::<Vec<_>>(),
                        "opinion": { "verdict": "pass", "reason": "Recorded only." } })
}

/// An answer of 200 with this body.
fn ok(body: &serde_json::Value) -> Vec<u8> {
    let body = body.to_string();
    format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

/// An answer of 200 whose turn calls `submit_verdict` once, with each checklist item
/// of source-fidelity given this disposition, confidently, and a reason that says
/// `said`; reporting this usage.
pub fn submission(disposition: &str, said: &str) -> Vec<u8> {
    submission_by(ANTHROPIC, disposition, said)
}

/// The same, as this provider answers.
pub fn submission_by(provider: Provider, disposition: &str, said: &str) -> Vec<u8> {
    let input = verdict_input(disposition, said);
    let body = match provider.name {
        "google" => serde_json::json!({
            "candidates": [{ "content": { "role": "model", "parts": [
                { "functionCall": { "id": "call_1", "name": "submit_verdict", "args": input },
                  "thoughtSignature": "c2lnbmVk" }
            ] }, "finishReason": "STOP" }],
            "usageMetadata": { "promptTokenCount": 3_000, "candidatesTokenCount": 400 },
            "responseId": "resp_2"
        }),
        "openai" => serde_json::json!({
            "id": "resp_2", "status": "completed",
            "output": [
                { "type": "reasoning", "id": "rs_1", "summary": [], "encrypted_content": "c2lnbmVk" },
                { "type": "function_call", "id": "fc_1", "call_id": "call_1",
                  "name": "submit_verdict", "arguments": input.to_string(), "status": "completed" }
            ],
            "usage": { "input_tokens": 3_000, "output_tokens": 400 }
        }),
        _ => serde_json::json!({
            "id": "msg_2", "type": "message", "role": "assistant", "model": "claude-haiku-5-5",
            "content": [
                { "type": "thinking", "thinking": "", "signature": "c2lnbmVk" },
                { "type": "tool_use", "id": "toolu_1", "name": "submit_verdict", "caller": { "type": "direct" },
                  "input": input }
            ],
            "stop_reason": "tool_use",
            "usage": { "input_tokens": 3_000, "output_tokens": 400 }
        }),
    };
    ok(&body)
}

/// An answer of 200 whose turn, as Anthropic answers, calls `submit_verdict` once with
/// each of these checklist items given this disposition, confidently, and a reason that
/// says `said`.
pub fn submission_of(items: &[&str], disposition: &str, said: &str) -> Vec<u8> {
    let input = verdict_input_of(items, disposition, said);
    ok(&serde_json::json!({
        "id": "msg_2", "type": "message", "role": "assistant", "model": "claude-haiku-5-5",
        "content": [
            { "type": "thinking", "thinking": "", "signature": "c2lnbmVk" },
            { "type": "tool_use", "id": "toolu_1", "name": "submit_verdict", "caller": { "type": "direct" },
              "input": input }
        ],
        "stop_reason": "tool_use",
        "usage": { "input_tokens": 3_000, "output_tokens": 400 }
    }))
}

/// An answer of 200 whose turn, as Anthropic answers, calls `submit_verdict` once with
/// these arguments.
pub fn submitting(input: serde_json::Value) -> Vec<u8> {
    ok(&serde_json::json!({
        "id": "msg_2", "type": "message", "role": "assistant", "model": "claude-haiku-5-5",
        "content": [
            { "type": "thinking", "thinking": "", "signature": "c2lnbmVk" },
            { "type": "tool_use", "id": "toolu_1", "name": "submit_verdict", "caller": { "type": "direct" },
              "input": input }
        ],
        "stop_reason": "tool_use",
        "usage": { "input_tokens": 3_000, "output_tokens": 400 }
    }))
}

/// An answer of 200 of this provider whose turn says `text` and ends, reporting this
/// usage: the whole input, how much of it was read from a cache, and the output.
pub fn said_by(provider: Provider, text: &str, usage: (u64, u64, u64)) -> Vec<u8> {
    let (input, cached, output) = usage;
    let body = match provider.name {
        "google" => serde_json::json!({
            "candidates": [{ "content": { "role": "model", "parts": [{ "text": text }] },
                             "finishReason": "STOP" }],
            "usageMetadata": { "promptTokenCount": input, "cachedContentTokenCount": cached,
                               "candidatesTokenCount": output * 4 / 5,
                               "thoughtsTokenCount": output - output * 4 / 5 },
            "responseId": "resp_1"
        }),
        "openai" => serde_json::json!({
            "id": "resp_1", "status": "completed",
            "output": [{ "type": "message", "id": "msg_1", "role": "assistant", "status": "completed",
                         "content": [{ "type": "output_text", "text": text, "annotations": [] }] }],
            "usage": { "input_tokens": input, "output_tokens": output,
                       "input_tokens_details": { "cached_tokens": cached } }
        }),
        _ => serde_json::json!({
            "id": "msg_1", "type": "message", "role": "assistant", "model": "claude-haiku-5-5",
            "content": [{ "type": "text", "text": text }], "stop_reason": "end_turn",
            "usage": { "input_tokens": input - cached, "output_tokens": output,
                       "cache_read_input_tokens": cached }
        }),
    };
    ok(&body)
}

/// The classes a check of source-fidelity sends: its words, the two pictures, and
/// what a tool returns.
pub const CHECK_CLASSES: [&str; 4] = ["source_images", "derived_images", "prompts", "evidence"];

/// A source and an edit of it in the world's store: their identities.
pub fn pair(judged: &Judged) -> (String, String) {
    let scratch = judged.world.beside("pictures");
    std::fs::create_dir_all(&scratch).unwrap();
    let source = import_picture(&judged.world.root, &scratch, "source", 10);
    let candidate = import_picture(&judged.world.root, &scratch, "candidate", 200);
    (source, candidate)
}

/// `whizbang squint check source-fidelity` of the world's pair through the binary's
/// door, under a judge at a server on loopback that gives these answers: what the door
/// prints or its complaint, and what the server saw.
pub fn checked(judged: &Judged, answers: Vec<Vec<u8>>) -> (Result<String, String>, Log) {
    checked_under(judged, answers, None)
}

/// As `checked`, with a brief given, as `--brief` gives one.
pub fn checked_under(
    judged: &Judged,
    answers: Vec<Vec<u8>>,
    brief: Option<&str>,
) -> (Result<String, String>, Log) {
    checked_as(
        judged,
        answers,
        brief,
        whizbang::door::Details::None,
        "full",
    )
}

/// As `checked_under`, with what is measured first and the projection of `align`, as
/// `--details` and `--projection` give them.
pub fn checked_as(
    judged: &Judged,
    answers: Vec<Vec<u8>>,
    brief: Option<&str>,
    details: whizbang::door::Details,
    projection: &str,
) -> (Result<String, String>, Log) {
    checked_measuring(judged, answers, brief, details, None, projection)
}

/// As `checked_as`, with the tools measured first named, as `--detail` names them.
pub fn checked_measuring(
    judged: &Judged,
    answers: Vec<Vec<u8>>,
    brief: Option<&str>,
    details: whizbang::door::Details,
    detail: Option<&str>,
    projection: &str,
) -> (Result<String, String>, Log) {
    let (source, candidate) = pair(judged);
    let (listener, log) = serving(&judged.pki, answers);
    let origin = format!("https://127.0.0.1:{}", listener.port());
    let over = judged.over_with(
        &judged.address(listener.port()),
        Some(&origin),
        &CHECK_CLASSES,
        "",
    );
    let executor = judged.runner(&over);
    let asked = whizbang::door::CheckAsked {
        use_case: "source-fidelity",
        source: Some(&source),
        candidate: &candidate,
        references: &[],
        judge: None,
        brief,
        request_id: None,
        details,
        detail,
        projection,
    };
    let root = judged.world.root.clone();
    let printed = whizbang::door::check(&executor, &asked, &|run| {
        whizbang::door::verdict(&root, run)
    });
    drop(executor);
    listener.finish();
    (printed, log)
}

/// `whizbang squint check` of a job and its images, in its order, through the binary's
/// door, under a judge at a server on loopback that gives these answers: what the door
/// prints or its complaint, and what the server saw.
pub fn checked_job(
    judged: &Judged,
    job: &whizbang_squint::job::Job,
    images: &[whizbang_core::artifact::ArtifactId],
    answers: Vec<Vec<u8>>,
) -> (Result<String, String>, Log) {
    let (listener, log) = serving(&judged.pki, answers);
    let origin = format!("https://127.0.0.1:{}", listener.port());
    let over = judged.over_with(
        &judged.address(listener.port()),
        Some(&origin),
        &CHECK_CLASSES,
        "",
    );
    let executor = judged.runner(&over);
    let root = judged.world.root.clone();
    let read = |run: &_| whizbang::door::verdict(&root, run);
    let measuring = whizbang::door::Measuring {
        details: whizbang::door::Details::None,
        detail: None,
        projection: "full",
        tools: &[],
        second_look: false,
    };
    let printed = whizbang::door::check_job(&executor, job, images, None, None, &measuring, &read);
    drop(executor);
    listener.finish();
    (printed, log)
}

/// The checklist of a job, by its items' ids.
pub fn checklist_of(job: &whizbang_squint::job::Job) -> Vec<String> {
    use whizbang_squint::use_case::{Checklist, resolve};
    let checklist = Checklist::build(&resolve(job).unwrap(), job);
    checklist.ids().map(str::to_owned).collect()
}
