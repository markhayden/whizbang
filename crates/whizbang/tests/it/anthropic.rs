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

/// A turn as the provider answers one.
pub fn a_turn() -> Vec<u8> {
    let body = serde_json::json!({
        "id": "msg_1", "type": "message", "role": "assistant", "model": "claude-haiku-5-5",
        "content": [{ "type": "text", "text": "Seen." }],
        "stop_reason": "end_turn", "usage": { "input_tokens": 9, "output_tokens": 2 }
    })
    .to_string();
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
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

/// A world whose config names one judge of the kind `service`, at `address`, with the
/// key in its env file, and approves `approved` for prompts when it names one.
pub struct Judged {
    pub world: World,
    pub key: String,
    pub pki: Arc<Pki>,
}

impl Judged {
    pub fn new(label: &str) -> Judged {
        let world = World::new(label);
        let reset = whizbang(&["spend", "reset", "--allowance", "20"], &world.root);
        assert_eq!(reset.code, Some(0), "{}", reset.err);
        Judged {
            world,
            key: made_up_key(),
            pki: Arc::new(Pki::loopback()),
        }
    }

    /// What the runner is built over: the config and the env file, written; the kind
    /// at `address`; and the sender trusting the test's root.
    pub fn over(&self, address: &str, approved: Option<&str>) -> Over {
        let mut config = format!(
            "[backends.judge]\nkind = \"service\"\nprovider = \"anthropic\"\n\
             model = \"claude-haiku-5-5\"\nkey_env = \"{KEY_VARIABLE}\"\n\n\
             [backends.judge.settings]\nat_most_micros = 250000\n"
        );
        if let Some(origin) = approved {
            config.push_str(&format!(
                "\n[egress.recipients.service]\norigin = \"{origin}\"\nclasses = [\"prompts\"]\n"
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

#[test]
fn the_key_goes_only_to_the_approved_origin_in_its_header() {
    use whizbang_core::contract::{Call, Executor};
    let judged = Judged::new("anthropic-key");
    let (listener, log) = serving(&judged.pki, vec![a_turn()]);
    let origin = format!("https://127.0.0.1:{}", listener.port());
    let over = judged.over(&format!("{origin}/v1/messages"), Some(&origin));
    let executor = judged.runner(&over);
    let ran = executor.submit(&Call::new("judges")).unwrap();
    assert_eq!(ran.outcome().name(), "completed");
    drop(executor);
    assert_eq!(listener.finish(), 1);
    let (head, body) = log.lock().unwrap()[0].clone();
    // In its one header, once, and nowhere else of the request.
    let line = format!("{KEY_HEADER}: {}\r\n", judged.key);
    assert_eq!(head.matches(&judged.key).count(), 1, "{head}");
    assert!(head.contains(&line), "{head}");
    let body = String::from_utf8(body).unwrap();
    assert!(!body.contains(&judged.key));
    assert!(head.starts_with("POST /v1/messages HTTP/1.1\r\n"), "{head}");
    assert!(head.contains("anthropic-version: 2023-06-01\r\n"), "{head}");
    // The store holds no byte of it.
    assert!(!judged.store_holds_the_key());
}
