//! The engine API in process, for its negative tests (`engine_guard`): the core's
//! executor over a store on the test kit's disk, the tests' own actions, and a door of
//! the tests' that keeps uploads in memory. Requests are bytes on a loopback socket.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Map, Value as Json, json};
use whizbang_core::artifact::ArtifactType;
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Call, Context, Engine, Failure, InputPort, Kind,
    NoBackends, Outcome, PreparedCall, Produced, Registry, RunId, ValueType,
};
use whizbang_core::identity::Digest;
use whizbang_core::store::Store;
use whizbang_serve::engine::{Host, Hosted, Refused, Secret, Settings, Stopped};
use whizbang_serve::play::guard::dimensions;
use whizbang_serve::tools::{self, Tool};
use whizbang_serve::{Door, Said};
use whizbang_testkit::fs_shim::ShimFs;
use whizbang_testkit::scratch::Scratch;

use crate::{Answer, parse};

/// The token of every engine the tests start: made up, and long enough.
pub const TOKEN: &str = "made-up-engine-token-0123456789abcdef";

/// The commit of a run of `held` stops in its hook until the test lets it go.
#[derive(Default)]
pub struct Gate {
    state: Mutex<(bool, bool)>,
    changed: Condvar,
}

impl Gate {
    pub fn enter_and_wait(&self) {
        let mut state = self.state.lock().unwrap();
        state.0 = true;
        self.changed.notify_all();
        let limit = Instant::now() + Duration::from_secs(30);
        while !state.1 && Instant::now() < limit {
            state = self
                .changed
                .wait_timeout(state, Duration::from_millis(50))
                .unwrap()
                .0;
        }
    }

    pub fn entered(&self) {
        let mut state = self.state.lock().unwrap();
        let limit = Instant::now() + Duration::from_secs(20);
        while !state.0 {
            assert!(Instant::now() < limit, "the commit was never held");
            state = self
                .changed
                .wait_timeout(state, Duration::from_millis(50))
                .unwrap()
                .0;
        }
    }

    pub fn release(&self) {
        self.state.lock().unwrap().1 = true;
        self.changed.notify_all();
    }
}

type Body = Box<dyn Fn(&Context) -> Result<Produced, Failure> + Send + Sync>;

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
    draft: whizbang_core::contract::Draft,
    body: impl Fn(&Context) -> Result<Produced, Failure> + Send + Sync + 'static,
) -> Arc<dyn Action> {
    Arc::new(Scripted(draft.build().unwrap(), Box::new(body)))
}

fn plain(name: &str) -> whizbang_core::contract::Draft {
    ActionDescriptor::draft(name, "1.0.0", Kind::Primitive, "A test action.")
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

/// The tests' door: each action by its descriptor, as the stdio server's tools; uploads
/// kept in memory by the digest of their bytes.
pub struct TestHosted {
    descriptors: Vec<ActionDescriptor>,
    pictures: Mutex<BTreeMap<String, Kept>>,
}

/// An upload kept: its bytes, its width and its height.
type Kept = (Vec<u8>, u32, u32);

impl Door for TestHosted {
    fn tools(&self) -> Vec<Tool> {
        self.descriptors.iter().map(tools::tool_of).collect()
    }

    fn call(
        &self,
        name: &str,
        arguments: &Map<String, Json>,
    ) -> Result<(Call, Option<String>), String> {
        let descriptor = (self.descriptors.iter())
            .find(|d| d.name() == name)
            .ok_or("the server offers no tool of that name")?;
        tools::call_of(descriptor, arguments)
    }

    fn said(&self, _: &str, run: &RunId, outcome: &Outcome) -> Said {
        Said {
            text: format!("run {} {}", run.as_str(), outcome.name()),
            error: outcome.name() != "completed",
        }
    }
}

impl Hosted for TestHosted {
    fn upload(&self, bytes: &[u8], _: u64) -> Result<Json, Refused> {
        let (width, height) = dimensions(bytes).ok_or(Refused::new(400, "no picture"))?;
        let id = Digest::of_bytes(bytes).to_string();
        let kept = (bytes.to_vec(), width, height);
        self.pictures.lock().unwrap().insert(id.clone(), kept);
        Ok(json!({ "image": id }))
    }

    fn mask(&self, bytes: &[u8], _: &str, _: u64) -> Result<Json, Refused> {
        let made = self.upload(bytes, 0)?;
        Ok(json!({ "mask": made["image"] }))
    }

    fn image_size(&self, image: &str) -> Option<(u32, u32)> {
        let pictures = self.pictures.lock().unwrap();
        pictures
            .get(image)
            .map(|(_, width, height)| (*width, *height))
    }

    fn artifact(&self, id: &str) -> Result<(&'static str, Vec<u8>), Refused> {
        let pictures = self.pictures.lock().unwrap();
        let (bytes, _, _) = pictures
            .get(id)
            .ok_or(Refused::new(404, "no artifact has that identity"))?;
        Ok(("image/png", bytes.clone()))
    }
}

/// An engine over a store of its own, and what a test needs to call it.
pub struct Engined {
    pub scratch: Scratch,
    pub fs: Arc<ShimFs>,
    pub host: Option<Host>,
    pub address: SocketAddr,
    pub executor: Arc<Engine>,
    pub released: Arc<AtomicBool>,
    pub gate: Arc<Gate>,
    /// How many runs each action started, by its name.
    pub ran: Arc<Mutex<BTreeMap<String, usize>>>,
}

impl Engined {
    pub fn start(label: &str, tune: impl FnOnce(&mut Settings)) -> Engined {
        let scratch = Scratch::new(label);
        let root = scratch.path().join("store");
        let fs = Arc::new(ShimFs::new());
        let store = Arc::new(Store::open(&root, fs.clone()).unwrap());
        let released = Arc::new(AtomicBool::new(false));
        let gate = Arc::new(Gate::default());
        let ran: Arc<Mutex<BTreeMap<String, usize>>> = Arc::default();
        let count = |name: &str| {
            let ran = ran.clone();
            let name = name.to_owned();
            move || *ran.lock().unwrap().entry(name.clone()).or_default() += 1
        };
        let mut registry = Registry::new();
        let counted = count("quick");
        registry
            .register(scripted(plain("quick"), move |context| {
                counted();
                context.events().progress("done at once");
                Ok(Produced::nothing())
            }))
            .unwrap();
        let counted = count("waits");
        registry
            .register(scripted(plain("waits"), move |context| {
                counted();
                context.events().progress("waiting for a stop");
                wait_for_stop(context)
            }))
            .unwrap();
        // Heeds its cancellation alone, and not its deadline: it ends only when the host
        // stops it, 30 seconds at most.
        registry
            .register(scripted(plain("heeds"), |context| {
                for _ in 0..6000 {
                    if context.cancellation().is_cancelled() {
                        return Err(Failure::error("stopped", "the run was cancelled"));
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                Ok(Produced::nothing())
            }))
            .unwrap();
        let deaf = released.clone();
        registry
            .register(scripted(plain("deaf"), move |_| {
                for _ in 0..6000 {
                    if deaf.load(Ordering::SeqCst) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                Ok(Produced::nothing())
            }))
            .unwrap();
        let (hooked, held) = (fs.clone(), gate.clone());
        registry
            .register(scripted(plain("held"), move |_| {
                let held = held.clone();
                hooked.pause_before(hooked.events().len(), move || held.enter_and_wait());
                Ok(Produced::nothing())
            }))
            .unwrap();
        let described = plain("described")
            .argument(Argument::optional("words", "Words.", ValueType::Text))
            .input(InputPort::optional(
                "image",
                ArtifactType::Image,
                "A picture.",
            ));
        registry
            .register(scripted(described, |_| Ok(Produced::nothing())))
            .unwrap();
        let descriptors = registry.descriptors().cloned().collect();
        let executor = Arc::new(Engine::over(
            registry,
            Arc::new(NoBackends),
            store.clone(),
            Digest::of_bytes(b""),
        ));
        let mut settings = Settings::new(SocketAddr::from(([127, 0, 0, 1], 0)));
        settings.grace = Duration::from_millis(400);
        tune(&mut settings);
        let hosted = Arc::new(TestHosted {
            descriptors,
            pictures: Mutex::new(BTreeMap::new()),
        });
        let host = Host::start(
            executor.clone(),
            hosted,
            settings,
            Secret::new(TOKEN).unwrap(),
        )
        .unwrap();
        Engined {
            address: host.address(),
            scratch,
            fs,
            host: Some(host),
            executor,
            released,
            gate,
            ran,
        }
    }

    /// The store's next change to the disk waits at the gate until the test lets it go.
    pub fn hold_next_publication(&self) {
        let gate = self.gate.clone();
        self.fs
            .pause_before(self.fs.events().len(), move || gate.enter_and_wait());
    }

    pub fn host_header(&self) -> String {
        format!("127.0.0.1:{}", self.address.port())
    }

    /// How many runs of `action` began.
    pub fn ran(&self, action: &str) -> usize {
        self.ran.lock().unwrap().get(action).copied().unwrap_or(0)
    }

    /// A request's bytes with the Host, the token, and JSON when a body is given.
    pub fn bytes(&self, method: &str, path: &str, body: Option<&Json>) -> Vec<u8> {
        let host = self.host_header();
        let bearer = format!("Bearer {TOKEN}");
        let mut headers = vec![
            ("Host", host.as_str()),
            ("Authorization", bearer.as_str()),
            ("MCP-Protocol-Version", "2025-06-18"),
        ];
        let body = body.map(|body| body.to_string().into_bytes());
        if body.is_some() {
            headers.push(("Content-Type", "application/json"));
        }
        crate::request(method, path, &headers, body.as_deref())
    }

    /// A call of a route, answered whole.
    pub fn api(&self, method: &str, path: &str, body: Option<&Json>) -> Answer {
        crate::send(self.address, &self.bytes(method, path, body))
    }

    /// An MCP message, answered whole.
    pub fn mcp(&self, message: &Json) -> Answer {
        self.api("POST", "/mcp", Some(message))
    }

    /// A `tools/call` of `tool` with these arguments, its stream held open: the
    /// connection, read to the end of its head.
    pub fn open_call(&self, tool: &str, arguments: Json) -> Opened {
        let message = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                             "params": {"name": tool, "arguments": arguments,
                                        "_meta": {"progressToken": "p"}}});
        self.open(&self.bytes("POST", "/mcp", Some(&message)))
    }

    /// Sends these bytes and reads the answer's head, leaving the connection open.
    pub fn open(&self, bytes: &[u8]) -> Opened {
        let mut conn = TcpStream::connect(self.address).unwrap();
        conn.write_all(bytes).unwrap();
        Opened::read_head(conn)
    }

    /// Polls a run until it says this state, 20 seconds at most.
    pub fn until_state(&self, run: &str, state: &str) -> Json {
        for _ in 0..4000 {
            let polled = self.api("GET", &format!("/v1/runs/{run}"), None);
            assert_eq!(polled.status, 200, "{}", polled.text());
            if polled.json()["state"] == state {
                return polled.json();
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("the run never said {state}");
    }

    pub fn stop(&mut self) -> Stopped {
        let host = self.host.take().unwrap();
        host.stopper().quit();
        host.wait()
    }
}

impl Drop for Engined {
    fn drop(&mut self) {
        self.released.store(true, Ordering::SeqCst);
        self.gate.release();
        if let Some(host) = self.host.take() {
            let stopper = host.stopper();
            stopper.quit();
            stopper.quit();
            host.wait();
        }
    }
}

/// A connection whose answer's head was read, kept open.
pub struct Opened {
    pub conn: TcpStream,
    pub status: u16,
    pub head: String,
    /// What came after the head, so far.
    pub body: Vec<u8>,
}

impl Opened {
    pub fn read_head(mut conn: TcpStream) -> Opened {
        conn.set_read_timeout(Some(Duration::from_secs(20)))
            .unwrap();
        let mut bytes = Vec::new();
        let mut piece = [0u8; 4096];
        while !bytes.windows(4).any(|w| w == b"\r\n\r\n") {
            match conn.read(&mut piece) {
                Ok(0) | Err(_) => break,
                Ok(read) => bytes.extend_from_slice(&piece[..read]),
            }
        }
        let answer = parse(&bytes);
        Opened {
            conn,
            status: answer.status,
            head: answer.head,
            body: answer.body,
        }
    }

    pub fn header(&self, name: &str) -> Option<String> {
        Answer {
            status: self.status,
            head: self.head.clone(),
            body: Vec::new(),
        }
        .header(name)
    }

    /// The run its head names.
    pub fn run(&self) -> String {
        self.header("whizbang-run").expect("a run named")
    }

    /// The rest of the stream, to its end.
    pub fn rest(mut self) -> String {
        let mut rest = Vec::new();
        let _ = self.conn.read_to_end(&mut rest);
        self.body.extend_from_slice(&rest);
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// The JSON-RPC messages of an event stream, in order.
pub fn events(stream: &str) -> Vec<Json> {
    stream
        .split("\n\n")
        .filter_map(|event| {
            let data = event.lines().find_map(|line| line.strip_prefix("data: "))?;
            serde_json::from_str(data).ok()
        })
        .collect()
}

/// A request id no other request of this process has.
pub fn fresh_id() -> String {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    format!("app-{}", NEXT.fetch_add(1, Ordering::SeqCst))
}

/// A PNG whose header says this size, and that holds nothing else.
pub fn png_saying(width: u32, height: u32) -> Vec<u8> {
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
    png.extend_from_slice(&width.to_be_bytes());
    png.extend_from_slice(&height.to_be_bytes());
    png.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
    png
}

/// An upload's bytes.
pub fn upload_bytes(engined: &Engined, content_type: &str, body: &[u8], query: &str) -> Vec<u8> {
    let host = engined.host_header();
    let bearer = format!("Bearer {TOKEN}");
    crate::request(
        "POST",
        &format!("/v1/uploads{query}"),
        &[
            ("Host", host.as_str()),
            ("Authorization", bearer.as_str()),
            ("Content-Type", content_type),
        ],
        Some(body),
    )
}
