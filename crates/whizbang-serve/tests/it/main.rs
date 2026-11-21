//! The playground host on a real loopback socket (plan m5, task H1; ADR 0041): a request
//! is bytes written to the socket and an answer is read to its status line, its headers
//! and its body, with no client library. The executor is the core's, over a store in a
//! scratch folder and actions of the tests' own.

// Test helpers outside `#[test]` functions may unwrap; a failure there is a test failure.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod play_guard;

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde_json::{Map, Value as Json, json};
use whizbang_core::contract::{
    Action, ActionDescriptor, Call, Context, Engine, Failure, Kind, NoBackends, PreparedCall,
    Produced, Registry,
};
use whizbang_core::identity::Digest;
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;
use whizbang_serve::play::{Host, Play, Settings, Token};
use whizbang_serve::tools;
use whizbang_testkit::scratch::Scratch;

/// What an action of the tests' does.
type Body = Box<dyn Fn(&Context) -> Result<Produced, Failure> + Send + Sync>;

/// An action of the tests': its descriptor and what it does.
struct Scripted(ActionDescriptor, Body);

impl Action for Scripted {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }

    fn run(&self, _: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        (self.1)(context)
    }
}

fn scripted(
    name: &str,
    body: impl Fn(&Context) -> Result<Produced, Failure> + Send + Sync + 'static,
) -> Arc<dyn Action> {
    let draft = ActionDescriptor::draft(name, "1.0.0", Kind::Primitive, "A test action.");
    Arc::new(Scripted(draft.build().unwrap(), Box::new(body)))
}

/// Waits for its run's cancellation or deadline, looking every 5 ms, 30 seconds at most.
fn wait_for_stop(context: &Context) -> Result<Produced, Failure> {
    for _ in 0..6000 {
        if context.cancellation().is_cancelled() || context.deadline_passed() {
            return Err(Failure::error("stopped", "the run was told to stop"));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(Produced::nothing())
}

/// The tests' actions: `quick` ends at once; `waits` waits to be stopped; `deaf` ignores
/// its cancellation and ends when the test lets it (`released`), 30 seconds at most;
/// `nest` calls `waits` and waits for it.
pub fn actions(released: Arc<AtomicBool>) -> Vec<Arc<dyn Action>> {
    vec![
        scripted("quick", |context| {
            context.events().progress("done at once");
            Ok(Produced::nothing())
        }),
        scripted("waits", |context| {
            context.events().progress("waiting for a stop");
            wait_for_stop(context)
        }),
        scripted("deaf", move |_| {
            for _ in 0..6000 {
                if released.load(Ordering::SeqCst) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(Produced::nothing())
        }),
        scripted("nest", |context| {
            let child = context.executor().submit(&Call::new("waits"));
            match child {
                Ok(_) => Err(Failure::error("stopped", "the child ended")),
                Err(_) => Err(Failure::error("stopped", "the child was refused")),
            }
        }),
    ]
}

/// The door of the tests: each action by its descriptor, as the stdio server's tools.
pub struct TestPlay {
    descriptors: Vec<ActionDescriptor>,
}

impl Play for TestPlay {
    fn call(
        &self,
        action: &str,
        arguments: &Map<String, Json>,
    ) -> Result<(Call, Option<String>), String> {
        let descriptor = (self.descriptors.iter())
            .find(|d| d.name() == action)
            .ok_or_else(|| "no action of that name".to_owned())?;
        tools::call_of(descriptor, arguments)
    }

    fn hello(&self) -> Map<String, Json> {
        let mut said = Map::new();
        said.insert("backends".to_owned(), json!([]));
        said
    }
}

/// A host over a store of its own, and what a test needs to call it.
pub struct Running {
    pub scratch: Scratch,
    pub host: Option<Host>,
    pub address: SocketAddr,
    pub token: Token,
    pub executor: Arc<Engine>,
    pub released: Arc<AtomicBool>,
}

impl Running {
    /// A host on a free port, under these settings once its store is named.
    pub fn start(label: &str, tune: impl FnOnce(&mut Settings)) -> Running {
        Running::start_with(label, tune, |_, _| {})
    }

    /// The same, with the store and the registry given to `before` before the executor
    /// is built.
    pub fn start_with(
        label: &str,
        tune: impl FnOnce(&mut Settings),
        before: impl FnOnce(&Scratch, &mut Registry),
    ) -> Running {
        let scratch = Scratch::new(label);
        let root = scratch.path().join("store");
        let store = Arc::new(Store::open(&root, Arc::new(RealFs)).unwrap());
        let released = Arc::new(AtomicBool::new(false));
        let mut registry = Registry::new();
        for action in actions(released.clone()) {
            registry.register(action).unwrap();
        }
        before(&scratch, &mut registry);
        let descriptors = registry.descriptors().cloned().collect();
        let executor = Arc::new(Engine::over(
            registry,
            Arc::new(NoBackends),
            store,
            Digest::of_bytes(b""),
        ));
        let mut settings = Settings::new(root);
        settings.port = 0;
        tune(&mut settings);
        let token = Token::new().unwrap();
        let host = Host::start(
            executor.clone(),
            Arc::new(TestPlay { descriptors }),
            settings,
            token.clone(),
        )
        .unwrap();
        Running {
            address: host.address(),
            scratch,
            host: Some(host),
            token,
            executor,
            released,
        }
    }

    /// `Host: 127.0.0.1:<port>`.
    pub fn host_header(&self) -> String {
        format!("127.0.0.1:{}", self.address.port())
    }

    pub fn origin(&self) -> String {
        format!("http://127.0.0.1:{}", self.address.port())
    }

    /// A call of the API as the page makes it: the Host, the token, the Origin on a
    /// change, and a JSON body when there is one.
    pub fn api(&self, method: &str, path: &str, body: Option<&Json>) -> Answer {
        let host = self.host_header();
        let bearer = format!("Bearer {}", self.token.hex());
        let origin = self.origin();
        let mut headers = vec![("Host", host.as_str()), ("Authorization", bearer.as_str())];
        if method != "GET" {
            headers.push(("Origin", origin.as_str()));
        }
        let bytes = body.map(|body| body.to_string().into_bytes());
        if bytes.is_some() {
            headers.push(("Content-Type", "application/json"));
        }
        send(
            self.address,
            &request(method, path, &headers, bytes.as_deref()),
        )
    }

    /// Prepares and starts a call of `action`: the run's id.
    pub fn run(&self, action: &str) -> String {
        let prepared = self.api(
            "POST",
            "/api/prepare",
            Some(&json!({"action": action, "arguments": {}})),
        );
        assert_eq!(prepared.status, 200, "{}", prepared.text());
        let handle = prepared.json()["handle"].as_str().unwrap().to_owned();
        let started = self.api(
            "POST",
            "/api/runs",
            Some(&json!({"action": action, "arguments": {}, "handle": handle})),
        );
        assert_eq!(started.status, 201, "{}", started.text());
        started.json()["run"].as_str().unwrap().to_owned()
    }

    /// Polls the run until it says this state, 20 seconds at most.
    pub fn until_state(&self, run: &str, state: &str) -> Json {
        for _ in 0..4000 {
            let polled = self.api("GET", &format!("/api/runs/{run}"), None);
            assert_eq!(polled.status, 200, "{}", polled.text());
            if polled.json()["state"] == state {
                return polled.json();
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("the run never said {state}");
    }

    /// Asks the host to quit, and waits for it.
    pub fn quit(&mut self) -> whizbang_serve::play::Stopped {
        let host = self.host.take().unwrap();
        host.stopper().quit();
        host.wait()
    }

    /// Waits for a host that was asked to quit another way.
    pub fn finish(&mut self) -> whizbang_serve::play::Stopped {
        self.host.take().unwrap().wait()
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.released.store(true, Ordering::SeqCst);
        if let Some(host) = self.host.take() {
            let stopper = host.stopper();
            stopper.quit();
            stopper.quit();
            host.wait();
        }
    }
}

/// A request's bytes.
pub fn request(method: &str, path: &str, headers: &[(&str, &str)], body: Option<&[u8]>) -> Vec<u8> {
    let mut head = format!("{method} {path} HTTP/1.1\r\n");
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    if let Some(body) = body {
        head.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    head.push_str("\r\n");
    let mut bytes = head.into_bytes();
    bytes.extend_from_slice(body.unwrap_or_default());
    bytes
}

/// An answer, read to its end.
#[derive(Debug, Clone)]
pub struct Answer {
    pub status: u16,
    /// The status line and the headers, as sent.
    pub head: String,
    pub body: Vec<u8>,
}

impl Answer {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn json(&self) -> Json {
        serde_json::from_slice(&self.body).unwrap_or(Json::Null)
    }

    /// The value of a header, its name matched without case.
    pub fn header(&self, name: &str) -> Option<String> {
        self.head.lines().skip(1).find_map(|line| {
            let (named, value) = line.split_once(':')?;
            named
                .eq_ignore_ascii_case(name)
                .then(|| value.trim().to_owned())
        })
    }

    /// The whole answer as text.
    pub fn whole(&self) -> String {
        format!("{}\r\n\r\n{}", self.head, self.text())
    }
}

/// Reads an answer from a connection to its end.
pub fn read_answer(stream: &mut TcpStream) -> Answer {
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    let mut bytes = Vec::new();
    let _ = stream.read_to_end(&mut bytes);
    parse(&bytes)
}

pub fn parse(bytes: &[u8]) -> Answer {
    let end = bytes
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .unwrap_or_else(|| panic!("no answer: {:?}", String::from_utf8_lossy(bytes)));
    let head = String::from_utf8_lossy(&bytes[..end]).into_owned();
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    Answer {
        status,
        head,
        body: bytes[end + 4..].to_vec(),
    }
}

/// Sends a request and reads its answer.
pub fn send(address: SocketAddr, bytes: &[u8]) -> Answer {
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    stream.write_all(bytes).unwrap();
    read_answer(&mut stream)
}
