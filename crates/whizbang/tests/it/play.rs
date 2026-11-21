//! `play::…`: `whizbang play` and its host (plan m5, task H1; ADR 0041). The binary is
//! started for what only a process shows (its address, `play.json`, a second launch, a
//! signal); the rest runs the host in process over the binary's wiring and the door the
//! binary hands it, with actions of the tests' own, over a store on the test kit's disk,
//! whose crash stands for the end of a process. Requests are bytes on a loopback socket.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value as Json, json};
use whizbang::playing::{Playground, read_hosting};
use whizbang::wiring::{self, Make, Over};
use whizbang_core::contract::{
    Action, ActionDescriptor, Call, Context, Executor, Failure, Kind, Outcome, PreparedCall,
    Produced, Registry, RunId, RunState,
};
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;
use whizbang_serve::play::{Host, Settings, Stopped, Token};
use whizbang_testkit::binary::Binary;
use whizbang_testkit::fs_shim::{Lost, ShimFs};
use whizbang_testkit::scratch::Scratch;

// The binary.

/// `whizbang play` started on a free port, printing its address, stopping at the end
/// of its input.
struct Played {
    child: Child,
    input: Option<ChildStdin>,
    _binary: Binary,
    port: u16,
    token: String,
}

fn play(store: &Path, more: &[&str]) -> Played {
    let mut binary = whizbang_testkit::binary::command(env!("CARGO_BIN_EXE_whizbang"));
    binary
        .args([
            "play",
            "--store",
            store.to_str().unwrap(),
            "--no-open",
            "--stdin-quit",
        ])
        .args(["--port", "0"])
        .args(more)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = binary.spawn().unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.as_mut().unwrap())
        .read_line(&mut line)
        .unwrap();
    if line.is_empty() {
        let mut err = String::new();
        child
            .stderr
            .as_mut()
            .unwrap()
            .read_to_string(&mut err)
            .unwrap();
        panic!("whizbang play printed no address: {err}");
    }
    let (port, token) = parsed_address(line.trim_end());
    Played {
        input: child.stdin.take(),
        child,
        _binary: binary,
        port,
        token,
    }
}

/// The port and the token of `http://127.0.0.1:<port>/#k=<token>`.
fn parsed_address(address: &str) -> (u16, String) {
    let rest = address.strip_prefix("http://127.0.0.1:").unwrap();
    let (port, token) = rest.split_once("/#k=").unwrap();
    (port.parse().unwrap(), token.to_owned())
}

impl Played {
    fn address(&self) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], self.port))
    }

    fn api(&self, method: &str, path: &str) -> Answer {
        api(self.address(), &self.token, method, path, None)
    }

    fn signal(&self, name: &str) {
        let sent = std::process::Command::new("kill")
            .args([format!("-{name}"), self.child.id().to_string()])
            .status()
            .unwrap();
        assert!(sent.success());
    }

    /// Waits for the process, 20 seconds at most: its exit code and what it printed.
    fn ended(mut self) -> (Option<i32>, String, String) {
        drop(self.input.take());
        let limit = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                let mut out = String::new();
                let mut err = String::new();
                let _ = self.child.stdout.take().unwrap().read_to_string(&mut out);
                let _ = self.child.stderr.take().unwrap().read_to_string(&mut err);
                return (status.code(), out, err);
            }
            if Instant::now() > limit {
                let _ = self.child.kill();
                panic!("whizbang play did not end");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Waits for the process without closing its input.
    fn exits_within(&mut self, limit: Duration) -> Option<i32> {
        let until = Instant::now() + limit;
        while Instant::now() < until {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status.code();
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.kill();
        panic!("whizbang play did not end within {limit:?}");
    }
}

/// The store takes a writer: whoever held it let it go.
fn takes_a_writer(root: &Path) {
    let opening = wiring::opening(root, Make::Nothing, wiring::recovery());
    assert!(opening.is_ok(), "{:?}", opening.err());
}

#[test]
fn hello_answers_the_version_and_the_store_behind_the_guard() {
    // One launch for three of the plan's cases (keeping the fast tier's time):
    // `play::hello_answers_the_version_and_the_store_behind_the_guard`,
    // `play::play_json_is_owner_only_and_gone_after_a_quit` and
    // `play::an_interrupt_signal_quits_as_the_page_does`. The quit is an interrupt from
    // the terminal, which ends as the page's Quit does (the page's own is
    // `play::a_quit_cancels_every_run_waits_and_releases_the_store`).
    let scratch = Scratch::new("play-hello");
    let root = scratch.path().join("store");
    let mut played = play(&root, &[]);
    let hello = played.api("GET", "/api/hello");
    assert_eq!(hello.status, 200, "{}", hello.text());
    assert_eq!(hello.json()["version"], env!("CARGO_PKG_VERSION"));
    assert!(hello.json()["store"].as_str().unwrap().ends_with("store"));
    assert_eq!(hello.json()["backends"], json!([]));
    // Behind the guard: no token, no answer.
    let bare = send(
        played.address(),
        format!(
            "GET /api/hello HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\r\n",
            played.port
        )
        .as_bytes(),
    );
    assert_eq!(bare.status, 401);
    assert!(!bare.text().contains("store"));
    // `play.json`: this user's alone, naming the port, the process and the token.
    let file = root.join("play.json");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&file).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "{mode:o}");
    }
    let hosting = read_hosting(&root).unwrap();
    assert_eq!(hosting.port, played.port);
    assert_eq!(hosting.process, played.child.id());
    assert_eq!(hosting.token, played.token);
    // An interrupt: the same quit, the file gone, the store free.
    played.signal("INT");
    assert_eq!(played.exits_within(Duration::from_secs(10)), Some(0));
    let (_, out, _) = played.ended();
    assert!(out.contains("every run it started has ended"), "{out}");
    assert!(!file.exists());
    takes_a_writer(&root);
}

#[test]
fn a_second_play_on_a_held_store_opens_the_running_hosts_address() {
    let scratch = Scratch::new("play-second");
    let root = scratch.path().join("store");
    let first = play(&root, &[]);
    let mut binary = whizbang_testkit::binary::command(env!("CARGO_BIN_EXE_whizbang"));
    binary.args([
        "play",
        "--store",
        root.to_str().unwrap(),
        "--no-open",
        "--port",
        "0",
    ]);
    let second = binary.output().unwrap();
    let out = String::from_utf8_lossy(&second.stdout);
    assert_eq!(second.status.code(), Some(0), "{out}");
    let address = out.lines().next().unwrap();
    assert_eq!(parsed_address(address), (first.port, first.token.clone()));
    assert!(out.contains("already running"), "{out}");
    let (code, _, err) = first.ended();
    assert_eq!(code, Some(0), "{err}");

    // A store held by anything else is refused as today, naming the holder.
    let held = wiring::opening(&root, Make::Nothing, wiring::recovery()).unwrap();
    let mut binary = whizbang_testkit::binary::command(env!("CARGO_BIN_EXE_whizbang"));
    binary.args([
        "play",
        "--store",
        root.to_str().unwrap(),
        "--no-open",
        "--port",
        "0",
    ]);
    let refused = binary.output().unwrap();
    let err = String::from_utf8_lossy(&refused.stderr);
    assert_eq!(refused.status.code(), Some(1), "{err}");
    assert!(err.contains("is being written by process"), "{err}");
    drop(held);
}

#[test]
fn two_interrupts_and_an_interrupt_with_every_worker_stalled_each_quit() {
    // Every worker held by a client that sends half a request: the quit does not wait
    // on a worker, so two signals (a hang-up, then an interrupt) end the host long
    // before the workers' deadline. That a second quit ends the grace with a run still
    // going is `play::a_second_quit_ends_the_grace_at_once`.
    let scratch = Scratch::new("play-interrupts");
    let root = scratch.path().join("store");
    let mut played = play(&root, &[]);
    let mut stalled = Vec::new();
    for _ in 0..20 {
        let mut stream = TcpStream::connect(played.address()).unwrap();
        stream.write_all(b"GET / HTTP/1.1\r\nHo").unwrap();
        stalled.push(stream);
    }
    let started = Instant::now();
    played.signal("HUP");
    played.signal("INT");
    assert_eq!(played.exits_within(Duration::from_secs(10)), Some(0));
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "{:?}",
        started.elapsed()
    );
    assert!(!root.join("play.json").exists());
    takes_a_writer(&root);
    drop(stalled);
}

#[test]
fn the_token_is_never_on_the_command_line_that_opens_chrome() {
    let scratch = Scratch::new("play-opener");
    let token = Token::new().unwrap();
    let address = whizbang::playing::address(7341, Some(&token));
    let opener = whizbang::playing::write_opener(scratch.path(), &address).unwrap();
    for mac in [true, false] {
        let command = whizbang::playing::chrome_command(&opener, mac);
        let said: Vec<String> = std::iter::once(command.get_program())
            .chain(command.get_args())
            .map(|word| word.to_string_lossy().into_owned())
            .collect();
        assert!(
            !said.iter().any(|word| word.contains(&token.hex())),
            "{said:?}"
        );
        assert!(
            said.iter().any(|word| word.ends_with("play-open.html")),
            "{said:?}"
        );
    }
    // The page that sends Chrome on holds it, for its owner alone.
    let page = std::fs::read_to_string(&opener).unwrap();
    assert!(page.contains(&address), "{page}");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&opener).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "{mode:o}");
    }
}

#[test]
fn a_hosting_file_that_others_could_have_written_is_not_trusted() {
    let scratch = Scratch::new("play-trust");
    let root = scratch.path().join("store");
    drop(Store::open(&root, Arc::new(RealFs)).unwrap());
    let hosting = whizbang::playing::Hosting {
        port: 1,
        process: 1,
        token: Token::new().unwrap().hex(),
    };
    whizbang::playing::write_hosting(&root, &hosting).unwrap();
    assert_eq!(read_hosting(&root), Some(hosting.clone()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let set = |path: &Path, mode: u32| {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
        };
        let file = root.join("play.json");
        set(&file, 0o644);
        assert_eq!(read_hosting(&root), None, "a file others may read");
        set(&file, 0o600);
        set(&root, 0o770);
        assert_eq!(read_hosting(&root), None, "a folder others may write");
        set(&root, 0o700);
        assert_eq!(read_hosting(&root), Some(hosting));
    }
}

#[test]
fn a_config_naming_a_key_with_no_env_file_opens_and_says_no_key() {
    let scratch = Scratch::new("play-no-key");
    let root = scratch.path().join("store");
    let config = scratch.path().join("whizbang.toml");
    std::fs::write(
        &config,
        "[backends.judge]\nkind = \"service\"\nprovider = \"anthropic\"\n\
         model = \"claude-haiku-5-5\"\nkey_env = \"JUDGE_KEY\"\n",
    )
    .unwrap();
    let played = play(&root, &["--config", config.to_str().unwrap()]);
    let hello = played.api("GET", "/api/hello");
    assert_eq!(hello.status, 200, "{}", hello.text());
    let backends = hello.json()["backends"].clone();
    let judge = &backends.as_array().unwrap()[0];
    assert_eq!(judge["capability"], "judge");
    assert_eq!(judge["ready"], false);
    assert_eq!(judge["missing"], "no_key");
    assert!(!hello.text().contains("JUDGE_KEY="), "{}", hello.text());
    let (code, _, err) = played.ended();
    assert_eq!(code, Some(0), "{err}");
}

// The host in process.

/// The commit of a run of `held` stops in its hook until the test lets it go.
#[derive(Default)]
struct Gate {
    state: Mutex<(bool, bool)>,
    changed: Condvar,
}

impl Gate {
    fn enter_and_wait(&self) {
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

    fn entered(&self) {
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

    fn release(&self) {
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
    name: &str,
    body: impl Fn(&Context) -> Result<Produced, Failure> + Send + Sync + 'static,
) -> Arc<dyn Action> {
    let draft = ActionDescriptor::draft(name, "1.0.0", Kind::Primitive, "A test action.");
    Arc::new(Scripted(draft.build().unwrap(), Box::new(body)))
}

fn wait_for_stop(context: &Context) -> Result<Produced, Failure> {
    for _ in 0..6000 {
        if context.cancellation().is_cancelled() || context.deadline_passed() {
            return Err(Failure::error("stopped", "the run was told to stop"));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(Produced::nothing())
}

/// A host over the binary's runner and the binary's door, with the tests' actions.
struct Hosted {
    scratch: Scratch,
    fs: Arc<ShimFs>,
    host: Option<Host>,
    address: SocketAddr,
    token: Token,
    released: Arc<AtomicBool>,
    gate: Arc<Gate>,
}

impl Hosted {
    fn start(label: &str, tune: impl FnOnce(&mut Settings)) -> Hosted {
        let scratch = Scratch::new(label);
        let root = scratch.path().join("store");
        let fs = Arc::new(ShimFs::new());
        let store = Arc::new(Store::open(&root, fs.clone()).unwrap());
        let released = Arc::new(AtomicBool::new(false));
        let gate = Arc::new(Gate::default());
        let mut registry = Registry::new();
        registry
            .register(scripted("waits", |context| {
                context.events().progress("waiting for a stop");
                wait_for_stop(context)
            }))
            .unwrap();
        let deaf = released.clone();
        registry
            .register(scripted("deaf", move |_| {
                for _ in 0..6000 {
                    if deaf.load(Ordering::SeqCst) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                Ok(Produced::nothing())
            }))
            .unwrap();
        registry
            .register(scripted("nest", |context| {
                match context.executor().submit(&Call::new("waits")) {
                    Ok(_) => Err(Failure::error("stopped", "the child ended")),
                    Err(_) => Err(Failure::error("stopped", "the child was refused")),
                }
            }))
            .unwrap();
        let (hooked, held) = (fs.clone(), gate.clone());
        registry
            .register(scripted("held", move |_| {
                let held = held.clone();
                hooked.pause_before(hooked.events().len(), move || held.enter_and_wait());
                Ok(Produced::nothing())
            }))
            .unwrap();
        let descriptors = registry.descriptors().cloned().collect();
        let runner = wiring::runner(registry, store, &Over::nothing()).unwrap();
        let play = Playground::new(descriptors, Arc::new(Over::nothing()));
        let mut settings = Settings::new(root);
        settings.port = 0;
        settings.grace = Duration::from_millis(400);
        tune(&mut settings);
        let token = Token::new().unwrap();
        let host = Host::start(Arc::new(runner), Arc::new(play), settings, token.clone()).unwrap();
        Hosted {
            address: host.address(),
            scratch,
            fs,
            host: Some(host),
            token,
            released,
            gate,
        }
    }

    fn root(&self) -> PathBuf {
        self.scratch.path().join("store")
    }

    fn api(&self, method: &str, path: &str, body: Option<&Json>) -> Answer {
        let origin = format!("http://127.0.0.1:{}", self.address.port());
        let origin = (method != "GET").then_some(origin.as_str());
        let bytes = request(self.address, &self.token.hex(), method, path, origin, body);
        send(self.address, &bytes)
    }

    fn run(&self, action: &str) -> String {
        let call = json!({"action": action, "arguments": {}});
        let prepared = self.api("POST", "/api/prepare", Some(&call));
        assert_eq!(prepared.status, 200, "{}", prepared.text());
        let mut start = call;
        start["handle"] = prepared.json()["handle"].clone();
        start["request_id"] = json!(fresh_id());
        let started = self.api("POST", "/api/runs", Some(&start));
        assert_eq!(started.status, 201, "{}", started.text());
        started.json()["run"].as_str().unwrap().to_owned()
    }

    fn until_state(&self, run: &str, state: &str) -> Json {
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

    fn wait(&mut self) -> Stopped {
        self.host.take().unwrap().wait()
    }

    /// How each run ended, read by a new writer of the store, which recovers it first.
    fn outcomes_after_reopening(&self, runs: &[&str]) -> Vec<Outcome> {
        let opening = wiring::opening(&self.root(), Make::Nothing, wiring::recovery()).unwrap();
        let store = opening.writer().unwrap();
        let reader = crate::runner(Registry::new(), store);
        runs.iter()
            .map(
                |run| match reader.status(&RunId::parse(run).unwrap()).unwrap().state() {
                    RunState::Ended(outcome) => outcome.clone(),
                    other => panic!("{run} has not ended: {other:?}"),
                },
            )
            .collect()
    }
}

impl Drop for Hosted {
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

#[test]
fn a_quit_cancels_every_run_waits_and_releases_the_store() {
    let mut hosted = Hosted::start("play-quit", |settings| {
        settings.grace = Duration::from_secs(10);
    });
    let runs = [hosted.run("waits"), hosted.run("waits")];
    for run in &runs {
        hosted.until_state(run, "running");
    }
    assert_eq!(hosted.api("POST", "/api/quit", None).status, 202);
    // From the quit on, nothing is taken.
    let late = hosted.api("GET", "/api/hello", None);
    assert_eq!(late.status, 503, "{}", late.text());
    let stopped = hosted.wait();
    assert_eq!(stopped.left, 0);
    // Each run was waited for, so the store is free at once, and each record says
    // cancelled.
    let runs: Vec<&str> = runs.iter().map(String::as_str).collect();
    let outcomes = hosted.outcomes_after_reopening(&runs);
    assert_eq!(outcomes, [Outcome::Cancelled, Outcome::Cancelled]);
}

#[test]
fn a_run_that_outlives_the_grace_is_counted_and_ended_as_interrupted_by_the_next_writer() {
    let mut hosted = Hosted::start("play-outlives", |_| {});
    let run = hosted.run("deaf");
    hosted.until_state(&run, "running");
    let started = Instant::now();
    hosted.host.as_ref().unwrap().stopper().quit();
    let stopped = hosted.wait();
    assert_eq!(stopped.left, 1);
    assert!(started.elapsed() >= Duration::from_millis(350));
    // The process ends here, with the run still going: its locks go with it.
    hosted.fs.crash(Lost::Unsynced);
    let outcomes = hosted.outcomes_after_reopening(&[&run]);
    match &outcomes[0] {
        Outcome::Error { kind, .. } => assert_eq!(kind, "interrupted"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_run_started_by_the_host_is_found_by_its_id_after_its_caller_is_gone() {
    let mut hosted = Hosted::start("play-found", |_| {});
    // Each request is a connection of its own, closed when it is answered: the caller
    // that started the run is gone.
    let run = hosted.run("waits");
    let open = hosted.api("GET", "/api/runs?open=1", None);
    let listed = open.json()["runs"].clone();
    assert_eq!(listed.as_array().unwrap().len(), 1);
    assert_eq!(listed[0]["run"], run.as_str());
    assert_eq!(listed[0]["action"], "waits");
    let polled = hosted.until_state(&run, "running");
    assert_eq!(polled["events"][0]["message"], "waiting for a stop");
    // A poll after the first event is answered with what is newer alone.
    let next = polled["next"].as_u64().unwrap();
    let after = hosted.api("GET", &format!("/api/runs/{run}?after={next}"), None);
    assert_eq!(after.json()["events"], json!([]));
    assert_eq!(
        hosted
            .api("POST", &format!("/api/runs/{run}/cancel"), None)
            .status,
        202
    );
    let ended = hosted.until_state(&run, "ended");
    assert_eq!(ended["outcome"], "cancelled");
    // An ended run is not listed as going, whether or not the keeper has looked yet:
    // the first list asked after the run was seen to end does not hold it.
    let open = hosted.api("GET", "/api/runs?open=1", None);
    assert_eq!(open.json()["runs"], json!([]), "{}", open.text());
    hosted.host.as_ref().unwrap().stopper().quit();
    assert_eq!(hosted.wait().left, 0);
}

#[test]
fn a_run_past_its_stated_time_is_cancelled_with_its_children() {
    let mut hosted = Hosted::start("play-stated", |settings| {
        settings.run_limit = Duration::from_millis(800);
    });
    let started = Instant::now();
    let run = hosted.run("nest");
    // The child is called before the stated time comes: its run is in the store.
    let limit = Instant::now() + Duration::from_secs(20);
    let root = hosted.root();
    while whizbang_core::store::Reader::open(&root, Arc::new(RealFs))
        .and_then(|reader| reader.snapshot())
        .map_or(0, |generation| generation.runs().len())
        < 2
    {
        assert!(Instant::now() < limit, "the child was never called");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        started.elapsed() < Duration::from_millis(800),
        "the child came after the stated time, on a machine too busy for this case"
    );
    let ended = hosted.until_state(&run, "ended");
    assert_eq!(ended["outcome"], "cancelled", "{ended}");
    assert!(started.elapsed() >= Duration::from_millis(780));
    hosted.host.as_ref().unwrap().stopper().quit();
    assert_eq!(hosted.wait().left, 0);
    // The child it called was cancelled with it.
    let reader = whizbang_core::store::Reader::open(&hosted.root(), Arc::new(RealFs)).unwrap();
    let runs: Vec<String> = reader.snapshot().unwrap().runs().keys().cloned().collect();
    assert_eq!(runs.len(), 2, "{runs:?}");
    let runs: Vec<&str> = runs.iter().map(String::as_str).collect();
    let outcomes = hosted.outcomes_after_reopening(&runs);
    assert_eq!(outcomes, [Outcome::Cancelled, Outcome::Cancelled]);
}

#[test]
fn a_run_that_ignores_its_cancellation_keeps_its_place_and_quit_still_exits_after_the_grace() {
    let mut hosted = Hosted::start("play-ignores", |settings| {
        settings.run_limit = Duration::from_millis(100);
        settings.calls = 1;
    });
    let run = hosted.run("deaf");
    // Its stated time passes: it is cancelled, and goes on.
    hosted.until_state(&run, "stopping");
    let open = hosted.api("GET", "/api/runs?open=1", None);
    assert_eq!(open.json()["runs"][0]["state"], "stopping");
    // It keeps its place: no other run is started while it goes.
    let call = json!({"action": "waits", "arguments": {}, "request_id": fresh_id()});
    let refused = hosted.api("POST", "/api/runs", Some(&call));
    assert_eq!(refused.status, 429, "{}", refused.text());
    let started = Instant::now();
    assert_eq!(hosted.api("POST", "/api/quit", None).status, 202);
    let stopped = hosted.wait();
    assert_eq!(stopped.left, 1);
    let took = started.elapsed();
    assert!(
        took >= Duration::from_millis(350) && took < Duration::from_secs(5),
        "{took:?}"
    );
}

#[test]
fn stop_pressed_often_while_a_run_is_in_its_final_publication_leaves_polling_and_quit_answered() {
    let mut hosted = Hosted::start("play-stop-often", |settings| {
        settings.grace = Duration::from_secs(10);
    });
    let run = hosted.run("held");
    hosted.gate.entered();
    // The canceller waits in the executor's cancel; no worker does.
    for _ in 0..20 {
        let started = Instant::now();
        let stopped = hosted.api("POST", &format!("/api/runs/{run}/cancel"), None);
        assert_eq!(stopped.status, 202);
        assert!(started.elapsed() < Duration::from_secs(2));
    }
    let polled = hosted.api("GET", &format!("/api/runs/{run}"), None);
    assert_eq!(polled.status, 200);
    assert_eq!(polled.json()["state"], "stopping");
    assert_eq!(hosted.api("GET", "/api/hello", None).status, 200);
    assert_eq!(hosted.api("POST", "/api/quit", None).status, 202);
    // Its commit goes on: a cancel in the commit step is too late, and the run
    // completes.
    hosted.gate.release();
    assert_eq!(hosted.wait().left, 0);
    let outcomes = hosted.outcomes_after_reopening(&[&run]);
    assert_eq!(outcomes[0].name(), "completed");
}

#[test]
fn a_quit_while_a_run_is_held_in_its_final_publication_exits_at_the_end_of_the_grace() {
    let mut hosted = Hosted::start("play-quit-held", |_| {});
    let run = hosted.run("held");
    hosted.gate.entered();
    let started = Instant::now();
    assert_eq!(hosted.api("POST", "/api/quit", None).status, 202);
    let stopped = hosted.wait();
    let took = started.elapsed();
    assert_eq!(stopped.left, 1, "{run}");
    assert!(
        took >= Duration::from_millis(350) && took < Duration::from_secs(5),
        "{took:?}"
    );
    hosted.gate.release();
}

#[test]
fn a_second_quit_ends_the_grace_at_once() {
    let mut hosted = Hosted::start("play-second-quit", |settings| {
        settings.grace = Duration::from_secs(10);
    });
    let run = hosted.run("deaf");
    hosted.until_state(&run, "running");
    let started = Instant::now();
    let stopper = hosted.host.as_ref().unwrap().stopper();
    stopper.quit();
    stopper.quit();
    let stopped = hosted.wait();
    assert_eq!(stopped.left, 1);
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn a_quit_while_a_start_is_admitted_cancels_the_run_it_makes() {
    let mut hosted = Hosted::start("play-quit-admitting", |settings| {
        settings.grace = Duration::from_secs(10);
    });
    let call = json!({"action": "waits", "arguments": {}});
    let prepared = hosted.api("POST", "/api/prepare", Some(&call));
    let mut start = call;
    start["handle"] = prepared.json()["handle"].clone();
    start["request_id"] = json!(fresh_id());
    // The admission's first change to the disk waits until the test lets it go.
    let gate = hosted.gate.clone();
    let paused = gate.clone();
    hosted
        .fs
        .pause_before(hosted.fs.events().len(), move || paused.enter_and_wait());
    let (address, token) = (hosted.address, hosted.token.hex());
    let starting = whizbang_testkit::threads::spawn(move || {
        let origin = format!("http://127.0.0.1:{}", address.port());
        send(
            address,
            &request(
                address,
                &token,
                "POST",
                "/api/runs",
                Some(&origin),
                Some(&start),
            ),
        )
    });
    gate.entered();
    hosted.host.as_ref().unwrap().stopper().quit();
    gate.release();
    let started = starting.join().unwrap();
    let stopped = hosted.wait();
    // Either the admission saw the quit's cancel and made no run, or it made one,
    // which was cancelled and waited for as every other run is: none is lost.
    let reader = whizbang_core::store::Reader::open(&hosted.root(), Arc::new(RealFs)).unwrap();
    let runs: Vec<String> = reader.snapshot().unwrap().runs().keys().cloned().collect();
    drop(reader);
    match runs.as_slice() {
        [] => assert_ne!(started.status, 201, "{}", started.text()),
        [run] => {
            assert_eq!(stopped.left, 0, "{}", started.text());
            let outcomes = hosted.outcomes_after_reopening(&[run]);
            assert_eq!(outcomes, [Outcome::Cancelled]);
        }
        more => panic!("{more:?}"),
    }
}

// Requests as bytes.

/// A request id no other request of this process has.
fn fresh_id() -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    format!("page-{}", NEXT.fetch_add(1, Ordering::SeqCst))
}

struct Answer {
    status: u16,
    body: Vec<u8>,
}

impl Answer {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    fn json(&self) -> Json {
        serde_json::from_slice(&self.body).unwrap_or(Json::Null)
    }
}

fn request(
    address: SocketAddr,
    token: &str,
    method: &str,
    path: &str,
    origin: Option<&str>,
    body: Option<&Json>,
) -> Vec<u8> {
    let mut head = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {token}\r\n",
        address.port()
    );
    if let Some(origin) = origin {
        head.push_str(&format!("Origin: {origin}\r\n"));
    }
    let body = body.map(|body| body.to_string());
    if let Some(body) = &body {
        head.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            body.len()
        ));
    }
    head.push_str("\r\n");
    head.push_str(body.as_deref().unwrap_or_default());
    head.into_bytes()
}

fn api(address: SocketAddr, token: &str, method: &str, path: &str, origin: Option<&str>) -> Answer {
    send(
        address,
        &request(address, token, method, path, origin, None),
    )
}

fn send(address: SocketAddr, bytes: &[u8]) -> Answer {
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    stream.write_all(bytes).unwrap();
    let mut answer = Vec::new();
    let _ = stream.read_to_end(&mut answer);
    let end = answer
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .unwrap_or_else(|| panic!("no answer: {:?}", String::from_utf8_lossy(&answer)));
    let status = String::from_utf8_lossy(&answer[..end])
        .split(' ')
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    Answer {
        status,
        body: answer[end + 4..].to_vec(),
    }
}
