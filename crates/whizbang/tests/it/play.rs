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
pub(crate) struct Played {
    child: Child,
    input: Option<ChildStdin>,
    _binary: Binary,
    port: u16,
    pub(crate) token: String,
}

pub(crate) fn play(store: &Path, more: &[&str]) -> Played {
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
    pub(crate) fn address(&self) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], self.port))
    }

    pub(crate) fn api(&self, method: &str, path: &str) -> Answer {
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
    pub(crate) fn ended(mut self) -> (Option<i32>, String, String) {
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

/// `judged`: a tool of the tests' that takes a picture and asks a judge, chosen by its
/// argument `judge`, and that plans. Its plan for the judge `keyless-judge` names a
/// capability the call does not need, which refuses the call with that choice alone: a
/// refusal about the choice itself (plan m7, task U1, item 4).
struct Judged(ActionDescriptor);

impl Action for Judged {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }

    fn plan(
        &self,
        planning: &whizbang_core::contract::Planning<'_>,
    ) -> Option<whizbang_core::contract::Plan> {
        let chosen = planning.argument("judge");
        let keyless = matches!(chosen, Some(whizbang_core::contract::Value::Text(text)) if text == "keyless-judge");
        let plan = whizbang_core::contract::Plan::new();
        Some(match keyless {
            true => plan.sending(whizbang_core::contract::Capability::Fill, 1),
            false => plan.because("a test's plan"),
        })
    }

    fn run(&self, _: &PreparedCall, _: &Context) -> Result<Produced, Failure> {
        Ok(Produced::nothing())
    }
}

fn judged() -> Arc<dyn Action> {
    use whizbang_core::contract::{Argument, InputPort, Requirement, ValueType};
    let draft = ActionDescriptor::draft("judged", "1.0.0", Kind::Primitive, "A test action.")
        .input(InputPort::required(
            "image",
            whizbang_core::artifact::ArtifactType::Image,
            "A picture.",
        ))
        .argument(Argument::optional("judge", "The judge.", ValueType::Text))
        .needs(Requirement::new(whizbang_core::contract::Capability::Judge).chosen_by("judge"))
        .plans();
    Arc::new(Judged(draft.build().unwrap()))
}

#[test]
fn a_choice_refused_for_itself_is_told_apart_from_a_missing_input() {
    let kitted = Kitted::new("play-u1-choice-refused");
    let choice = |prepared: &Json, value: &str| {
        let judges = prepared["choices"]["judge"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        judges
            .into_iter()
            .find(|choice| choice["value"] == value)
            .unwrap_or_else(|| panic!("no choice {value}: {prepared}"))
    };
    // With its picture: the call as given is ready, and the choice whose plan is no plan
    // is refused for itself, in the core's words.
    let given = kitted.prepare("judged", &json!({"image": kitted.picture(41)}));
    assert_eq!(given["ready"], true, "{given}");
    let refused = choice(&given, "keyless-judge");
    assert_eq!(refused["ready"], false, "{refused}");
    assert_eq!(refused["kind"], "refused", "{refused}");
    assert!(
        refused["why"]
            .as_str()
            .unwrap()
            .contains("no need of this call"),
        "{refused}"
    );
    assert_eq!(choice(&given, "cloud-judge")["kind"], "ready", "{given}");
    // The same with the refused choice the one the call names now: it is still refused
    // for itself, and not taken for the call's own refusal.
    let picked = kitted.prepare(
        "judged",
        &json!({"image": kitted.picture(42), "judge": "keyless-judge"}),
    );
    assert_eq!(picked["refused"], true, "{picked}");
    assert_eq!(
        choice(&picked, "keyless-judge")["kind"],
        "refused",
        "{picked}"
    );
    assert_eq!(choice(&picked, "cloud-judge")["kind"], "ready", "{picked}");
    // Without it: every choice is refused as the call is, for the missing input, and is
    // answered with its backend's own readiness instead.
    let missing = kitted.prepare("judged", &json!({}));
    assert_eq!(missing["refused"], true, "{missing}");
    assert_eq!(
        choice(&missing, "keyless-judge")["kind"],
        "no_key",
        "{missing}"
    );
    assert_eq!(
        choice(&missing, "cloud-judge")["kind"],
        "ready",
        "{missing}"
    );
    // The same, the choice named now included.
    let missing = kitted.prepare("judged", &json!({"judge": "keyless-judge"}));
    assert_eq!(
        choice(&missing, "keyless-judge")["kind"],
        "no_key",
        "{missing}"
    );
    assert_eq!(
        choice(&missing, "cloud-judge")["kind"],
        "ready",
        "{missing}"
    );
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
        registry.register(judged()).unwrap();
        registry
            .register(scripted("waits", |context| {
                context.events().progress("waiting for a stop");
                wait_for_stop(context)
            }))
            .unwrap();
        let deaf = released.clone();
        registry
            .register(scripted("deaf", move |context| {
                // Said first, so that a test knows its action has started: a run is
                // running from its admission, before that.
                context.events().progress("deaf to a cancel");
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

    pub(crate) fn root(&self) -> PathBuf {
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

    /// The run's poll once its first event is there. A run says it is running when its
    /// action starts, before the action has said anything: a poll that saw it running
    /// may hold no event yet.
    fn until_first_event(&self, run: &str) -> Json {
        for _ in 0..4000 {
            let polled = self.api("GET", &format!("/api/runs/{run}"), None);
            assert_eq!(polled.status, 200, "{}", polled.text());
            if !polled.json()["events"][0].is_null() {
                return polled.json();
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("the run never said anything");
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
    // Its action has started: a quit's cancel can no longer win before it, and end the
    // run as cancelled at once.
    hosted.until_first_event(&run);
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
    hosted.until_state(&run, "running");
    // Running is said before the action's first event, which comes after: waited for.
    let polled = hosted.until_first_event(&run);
    assert_eq!(polled["state"], "running");
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
pub(crate) fn fresh_id() -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    format!("page-{}", NEXT.fetch_add(1, Ordering::SeqCst))
}

pub(crate) struct Answer {
    pub(crate) status: u16,
    pub(crate) body: Vec<u8>,
}

impl Answer {
    pub(crate) fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub(crate) fn json(&self) -> Json {
        serde_json::from_slice(&self.body).unwrap_or(Json::Null)
    }
}

pub(crate) fn request(
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

pub(crate) fn send(address: SocketAddr, bytes: &[u8]) -> Answer {
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

// Running a task (plan m5, task W1): the host over the binary's registry and wiring, with
// the test kit's `playkit` backends under a config of the test's, in a store with an
// allowance; the same as the fixture host of the Chrome cases.

/// The config of the kit's backends: a segmenter that costs nothing, a filler and a
/// judge at addresses of the reserved `.example` domain, and a judge whose key is named
/// and not set. `fill_model` names the filler's model, so a test can change the config.
fn kit_config(fill_model: &str) -> String {
    let kind = whizbang_testkit::playkit::KIND;
    format!(
        "[backends.segment]\nkind = \"{kind}\"\nmodel = \"seg\"\nurl = \"https://segment.example.org/v1\"\n\n\
         [backends.fill]\nkind = \"{kind}\"\nmodel = \"{fill_model}\"\nurl = \"https://fill.example.org/v1\"\n\n\
         [backends.judge]\ndefault = \"cloud-judge\"\n\n\
         [backends.judge.cloud-judge]\nkind = \"{kind}\"\nmodel = \"judge\"\nurl = \"https://judge.example.org/v1\"\n\n\
         [backends.judge.keyless-judge]\nkind = \"{kind}\"\nmodel = \"keyless\"\n\
         url = \"https://judge.example.org/v2\"\nkey_env = \"PLAYKIT_TEST_KEY\"\n\n\
         [egress.recipients.segment]\norigin = \"https://segment.example.org\"\nclasses = [\"prompts\", \"source_images\"]\n\n\
         [egress.recipients.fill]\norigin = \"https://fill.example.org\"\nclasses = [\"source_images\", \"derived_images\"]\n\n\
         [egress.recipients.judge]\norigin = \"https://judge.example.org\"\n\
         classes = [\"prompts\", \"source_images\", \"derived_images\", \"evidence\"]\n"
    )
}

/// An action of the tests' whose one progress message holds an escape sequence, and
/// that takes some words, which it does nothing with: a call that is quick to run.
fn shouting() -> Arc<dyn Action> {
    let draft = ActionDescriptor::draft("shout", "1.0.0", Kind::Primitive, "A test action.")
        .argument(whizbang_core::contract::Argument::optional(
            "words",
            "Some words.",
            whizbang_core::contract::ValueType::Text,
        ));
    Arc::new(Scripted(
        draft.build().unwrap(),
        Box::new(|context: &Context| {
            context.events().progress("\u{1b}[2Jcleared \u{7} and rung");
            Ok(Produced::nothing())
        }),
    ))
}

/// The host over the kit, in a store of its own with ten dollars to spend.
pub(crate) struct Kitted {
    scratch: Scratch,
    door: Arc<Playground>,
    kit: Arc<whizbang_testkit::playkit::PlayKit>,
    host: Option<Host>,
    address: SocketAddr,
    token: Token,
}

impl Kitted {
    pub(crate) fn new(label: &str) -> Kitted {
        Kitted::over(Scratch::new(label), true)
    }

    /// A host over the store and config in `scratch`; a new store gets its allowance.
    fn over(scratch: Scratch, new: bool) -> Kitted {
        Kitted::built(scratch, new, false)
    }

    /// The same, with the decision cache of `check` on, as `whizbang play` turns it on
    /// when the config does (plan m5, task W3).
    fn cached(label: &str) -> Kitted {
        Kitted::built(Scratch::new(label), true, true)
    }

    /// A host over a config of the test's own, in a new store with its allowance.
    fn configured(label: &str, text: &str) -> Kitted {
        let scratch = Scratch::new(label);
        std::fs::write(scratch.path().join("whizbang.toml"), text).unwrap();
        Kitted::built(scratch, true, false)
    }

    fn built(scratch: Scratch, new: bool, cached: bool) -> Kitted {
        let root = scratch.path().join("store");
        let config = scratch.path().join("whizbang.toml");
        if new && !config.exists() {
            std::fs::write(&config, kit_config("fill")).unwrap();
        }
        let kit = whizbang_testkit::playkit::PlayKit::new();
        let mut kinds = wiring::kinds(&whizbang::pack::OneWorker::installed());
        kinds.register(kit.kind()).unwrap();
        let over = Over::nothing().config(Some(&config)).kinds(kinds);
        // The real disk without its syncs, which these tests do not test.
        let store = Arc::new(Store::open(&root, Arc::new(crate::checked::Unsynced)).unwrap());
        if new {
            let ten = whizbang_core::spend::Amount::new(10_000_000).unwrap();
            whizbang_core::spend::reset(&store, ten, 1).unwrap();
        }
        let mut registry = wiring::registry_over(&over, cached).unwrap();
        registry.register(shouting()).unwrap();
        // Says one message more than a run keeps.
        registry
            .register(scripted("chatty", |context| {
                let most = whizbang_core::contract::MESSAGES_KEPT_LIMIT;
                for at in 0..most + 3 {
                    context.events().progress(&format!("message {at}"));
                }
                Ok(Produced::nothing())
            }))
            .unwrap();
        registry.register(judged()).unwrap();
        registry
            .register(scripted("waits", |context| {
                context.events().progress("waiting for a stop");
                wait_for_stop(context)
            }))
            .unwrap();
        let descriptors = registry.descriptors().cloned().collect();
        let runner = wiring::runner(registry, store.clone(), &over).unwrap();
        let play = Arc::new(Playground::new(descriptors, Arc::new(over)).with_store(store, &root));
        let mut settings = Settings::new(root);
        settings.port = 0;
        settings.grace = Duration::from_millis(400);
        let token = Token::new().unwrap();
        let host = Host::start(Arc::new(runner), play.clone(), settings, token.clone()).unwrap();
        Kitted {
            address: host.address(),
            scratch,
            door: play,
            kit,
            host: Some(host),
            token,
        }
    }

    /// Stops the host and starts another over the same store and config.
    fn restarted(self) -> Kitted {
        self.restarted_after(|_| ())
    }

    /// The same, doing `between` to the store's folder while no host holds it.
    pub(crate) fn restarted_after(mut self, between: impl FnOnce(&Path)) -> Kitted {
        let host = self.host.take().unwrap();
        host.stopper().quit();
        host.wait();
        between(&self.root());
        let scratch = std::mem::replace(&mut self.scratch, Scratch::new("play-spent"));
        Kitted::over(scratch, false)
    }

    pub(crate) fn root(&self) -> PathBuf {
        self.scratch.path().join("store")
    }

    /// The script of the kit's fakes, which a test steers.
    pub(crate) fn kit(&self) -> &whizbang_testkit::playkit::PlayKit {
        &self.kit
    }

    pub(crate) fn api(&self, method: &str, path: &str, body: Option<&Json>) -> Answer {
        let origin = format!("http://127.0.0.1:{}", self.address.port());
        let origin = (method != "GET").then_some(origin.as_str());
        let bytes = request(self.address, &self.token.hex(), method, path, origin, body);
        send(self.address, &bytes)
    }

    /// A picture sent as the page sends one.
    pub(crate) fn upload(&self, png: &[u8], name: &str) -> Answer {
        let mut head = format!(
            "POST /api/pictures?name={name} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\
             Authorization: Bearer {token}\r\nOrigin: http://127.0.0.1:{port}\r\n\
             Content-Type: image/png\r\nContent-Length: {}\r\n\r\n",
            png.len(),
            port = self.address.port(),
            token = self.token.hex(),
        )
        .into_bytes();
        head.extend_from_slice(png);
        send(self.address, &head)
    }

    /// A picture of the store, by its identity.
    pub(crate) fn picture(&self, shade: u8) -> String {
        let sent = self.upload(&png(64, 48, shade), "porch.png");
        assert_eq!(sent.status, 201, "{}", sent.text());
        sent.json()["id"].as_str().unwrap().to_owned()
    }

    pub(crate) fn prepare(&self, action: &str, arguments: &Json) -> Json {
        let body = json!({"action": action, "arguments": arguments});
        let prepared = self.api("POST", "/api/prepare", Some(&body));
        assert_eq!(prepared.status, 200, "{}", prepared.text());
        prepared.json()
    }

    pub(crate) fn begin(&self, action: &str, arguments: &Json, handle: &Json, id: &str) -> Answer {
        let body = json!({"action": action, "arguments": arguments, "handle": handle,
                          "request_id": id});
        self.api("POST", "/api/runs", Some(&body))
    }

    pub(crate) fn until_ended(&self, run: &str) -> Json {
        for _ in 0..4000 {
            let polled = self.api("GET", &format!("/api/runs/{run}?after=0"), None);
            assert_eq!(polled.status, 200, "{}", polled.text());
            if polled.json()["state"] == "ended" {
                return polled.json();
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("the run never ended");
    }

    /// The store's generation as a reader of its own sees it.
    pub(crate) fn generation(&self) -> whizbang_core::store::Generation {
        let reader = whizbang_core::store::Reader::open(&self.root(), Arc::new(RealFs)).unwrap();
        reader.snapshot().unwrap()
    }
}

impl Drop for Kitted {
    fn drop(&mut self) {
        let _ = self.kit.set("reset");
        if let Some(host) = self.host.take() {
            let stopper = host.stopper();
            stopper.quit();
            stopper.quit();
            host.wait();
        }
    }
}

/// A PNG of this size, one shade.
pub(crate) fn png(width: u32, height: u32, shade: u8) -> Vec<u8> {
    use image::ImageEncoder;
    let rgb: Vec<u8> = (0..width * height)
        .flat_map(|at| [shade, (at % 251) as u8, 90])
        .collect();
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(&rgb, width, height, image::ExtendedColorType::Rgb8)
        .unwrap();
    png
}

/// The arguments of `shout`.
fn words(words: &str) -> Json {
    json!({ "words": words })
}

#[test]
fn a_start_without_a_matching_fresh_preparation_is_refused_as_stale() {
    let kitted = Kitted::new("play-w1-stale");
    let prepared = kitted.prepare("shout", &words("these"));
    assert_eq!(prepared["ready"], true, "{prepared}");
    let before = kitted.generation().runs().len();
    // Other arguments under this handle, and no handle: each refused before admission.
    for (arguments, handle) in [
        (words("others"), prepared["handle"].clone()),
        (words("these"), Json::Null),
    ] {
        let refused = kitted.begin("shout", &arguments, &handle, &fresh_id());
        assert_eq!(refused.status, 409, "{}", refused.text());
        assert_eq!(refused.json()["kind"], "stale", "{}", refused.text());
    }
    assert_eq!(kitted.generation().runs().len(), before);
}

#[test]
fn a_start_sent_twice_with_one_request_id_answers_one_run() {
    let kitted = Kitted::new("play-w1-twice");
    let arguments = words("twice");
    let prepared = kitted.prepare("shout", &arguments);
    let first = kitted.begin("shout", &arguments, &prepared["handle"], "page-twice");
    assert_eq!(first.status, 201, "{}", first.text());
    let again = kitted.begin("shout", &arguments, &prepared["handle"], "page-twice");
    assert_eq!(again.status, 200, "{}", again.text());
    assert_eq!(again.json()["run"], first.json()["run"]);
    assert_eq!(again.json()["repeat"], true);
    let ended = kitted.until_ended(first.json()["run"].as_str().unwrap());
    assert_eq!(ended["outcome"], "completed", "{ended}");
    assert_eq!(ended["action"], "shout");
    assert_eq!(ended["arguments"], arguments);
}

#[test]
fn a_repeat_after_the_handle_expired_or_the_host_restarted_answers_its_run() {
    let kitted = Kitted::new("play-w1-restart");
    let arguments = words("kept");
    let prepared = kitted.prepare("shout", &arguments);
    let first = kitted.begin("shout", &arguments, &prepared["handle"], "page-kept");
    assert_eq!(first.status, 201, "{}", first.text());
    kitted.until_ended(first.json()["run"].as_str().unwrap());
    // A new host, which holds neither the handle nor the id: the executor answers it.
    let kitted = kitted.restarted();
    let again = kitted.begin("shout", &arguments, &prepared["handle"], "page-kept");
    assert_eq!(again.status, 200, "{}", again.text());
    assert_eq!(again.json()["run"], first.json()["run"]);
}

#[test]
fn a_conflicting_request_under_one_id_is_refused_as_a_conflict() {
    let kitted = Kitted::new("play-w1-conflict");
    let arguments = words("one");
    let prepared = kitted.prepare("shout", &arguments);
    let first = kitted.begin("shout", &arguments, &prepared["handle"], "page-one");
    assert_eq!(first.status, 201, "{}", first.text());
    let other = words("another");
    let handle = kitted.prepare("shout", &other)["handle"].clone();
    let conflict = kitted.begin("shout", &other, &handle, "page-one");
    assert_eq!(conflict.status, 409, "{}", conflict.text());
    assert_eq!(conflict.json()["kind"], "conflict");
    // After a restart the host holds no copy of what was asked: the id the store holds
    // is answered with its run, whatever is sent under it (ADR 0041, "Runs belong to
    // the host").
    kitted.until_ended(first.json()["run"].as_str().unwrap());
    let kitted = kitted.restarted();
    let answered = kitted.begin("shout", &other, &Json::Null, "page-one");
    assert_eq!(answered.status, 200, "{}", answered.text());
    assert_eq!(answered.json()["run"], first.json()["run"]);
}

#[test]
fn a_repeat_is_answered_while_four_runs_are_going() {
    let kitted = Kitted::new("play-w1-four");
    let arguments = json!({});
    let mut runs = Vec::new();
    for at in 0..4 {
        let prepared = kitted.prepare("waits", &arguments);
        let started = kitted.begin(
            "waits",
            &arguments,
            &prepared["handle"],
            &format!("page-four-{at}"),
        );
        assert_eq!(started.status, 201, "{}", started.text());
        runs.push(started.json()["run"].clone());
    }
    let prepared = kitted.prepare("waits", &arguments);
    let repeat = kitted.begin("waits", &arguments, &prepared["handle"], "page-four-0");
    assert_eq!(repeat.status, 200, "{}", repeat.text());
    assert_eq!(repeat.json()["run"], runs[0]);
    let fifth = kitted.begin("waits", &arguments, &prepared["handle"], "page-four-5");
    assert_eq!(fifth.status, 429, "{}", fifth.text());
    // Stop each: the quit at the end waits for none.
    for run in &runs {
        let path = format!("/api/runs/{}/cancel", run.as_str().unwrap());
        assert_eq!(kitted.api("POST", &path, None).status, 202);
    }
}

#[test]
fn a_preparation_made_under_another_config_is_refused_as_stale() {
    let kitted = Kitted::new("play-w1-config");
    let arguments = json!({"image": kitted.picture(15), "prompt": "the hose"});
    let prepared = kitted.prepare("remove_prompt", &arguments);
    assert_eq!(prepared["ready"], true, "{prepared}");
    // The config changes between the preparation and the start: the filler's model.
    let config = kitted.scratch.path().join("whizbang.toml");
    std::fs::write(&config, kit_config("other-fill")).unwrap();
    let refused = kitted.begin(
        "remove_prompt",
        &arguments,
        &prepared["handle"],
        &fresh_id(),
    );
    assert_eq!(refused.status, 409, "{}", refused.text());
    assert_eq!(refused.json()["kind"], "stale", "{}", refused.text());
    assert_eq!(kitted.kit.requests(), 0);
}

#[test]
fn the_first_import_into_a_new_store_keeps_the_picture_and_its_name_in_one_publication() {
    let kitted = Kitted::new("play-w1-import");
    let before = kitted.generation().number();
    let sent = kitted.upload(&png(64, 48, 16), "kitchen%20wide.png");
    assert_eq!(sent.status, 201, "{}", sent.text());
    let id = sent.json()["id"].as_str().unwrap().to_owned();
    assert_eq!(sent.json()["name"], "kitchen wide.png");
    assert_eq!(
        (sent.json()["width"].clone(), sent.json()["height"].clone()),
        (json!(64), json!(48))
    );
    let generation = kitted.generation();
    assert_eq!(generation.number(), before + 1, "one publication");
    let hex = id.trim_start_matches("sha256:");
    let named = generation
        .entries()
        .keys()
        .any(|name| name == &format!("play/pictures/{hex}"));
    let held = generation
        .entries()
        .keys()
        .any(|name| name.contains(hex) && name.starts_with("artifacts/"));
    assert!(
        named && held,
        "{:?}",
        generation.entries().keys().collect::<Vec<_>>()
    );
    let listed = kitted.api("GET", "/api/pictures", None).json();
    assert_eq!(listed["pictures"][0]["id"], id.as_str());
    assert_eq!(listed["pictures"][0]["name"], "kitchen wide.png");
    // The picture is the import's: its canonical pixels as a PNG.
    let shown = kitted.api("GET", &format!("/api/artifacts/{id}"), None);
    assert_eq!(shown.status, 200);
    assert!(shown.body.starts_with(b"\x89PNG"));
    let thumb = kitted.api("GET", &format!("/api/artifacts/{id}/thumb?edge=16"), None);
    assert_eq!(thumb.status, 200, "{}", thumb.text());
}

#[test]
fn a_picture_that_fails_to_decode_leaves_nothing_in_the_store() {
    let kitted = Kitted::new("play-w1-undecoded");
    let before = kitted.generation();
    // A PNG whose header says a small size and whose data is cut short.
    let mut broken = png(64, 48, 17);
    broken.truncate(60);
    let refused = kitted.upload(&broken, "broken.png");
    assert_eq!(refused.status, 400, "{}", refused.text());
    let after = kitted.generation();
    assert_eq!(after.number(), before.number());
    assert_eq!(after.entries().len(), before.entries().len());
    let staging = kitted.root().join("staging");
    let staged = std::fs::read_dir(&staging).map_or(0, |dir| dir.count());
    assert_eq!(staged, 0);
}

#[test]
fn a_drawn_shape_is_committed_as_a_location_bound_to_its_picture() {
    let kitted = Kitted::new("play-w1-drawn");
    let picture = kitted.picture(18);
    let drawn = json!({"picture": picture,
                       "shape": {"kind": "box", "left": 4, "top": 6, "right": 20, "bottom": 30}});
    let made = kitted.api("POST", "/api/locations", Some(&drawn));
    assert_eq!(made.status, 201, "{}", made.text());
    let id = made.json()["id"].as_str().unwrap().to_owned();
    let shown = kitted
        .api("GET", &format!("/api/artifacts/{id}"), None)
        .json();
    assert_eq!(
        shown,
        json!({"kind": "box", "left": 4.0, "top": 6.0, "right": 20.0, "bottom": 30.0})
    );
    // Bound to the picture: a removal at it prepares, ready.
    let prepared = kitted.prepare("remove", &json!({"image": picture, "location": id}));
    assert_eq!(prepared["ready"], true, "{prepared}");
    // A stroke; a polygon of more than 64 points; a shape outside the picture.
    let stroke = json!({"picture": picture,
                        "shape": {"kind": "stroke", "points": [[2, 2], [30, 20]], "radius": 3}});
    assert_eq!(
        kitted.api("POST", "/api/locations", Some(&stroke)).status,
        201
    );
    let many: Vec<Json> = (0..65).map(|at| json!([at % 60, at % 40])).collect();
    let polygon = json!({"picture": picture, "shape": {"kind": "polygon", "points": many}});
    assert_eq!(
        kitted.api("POST", "/api/locations", Some(&polygon)).status,
        400
    );
    let outside = json!({"picture": picture,
                         "shape": {"kind": "box", "left": 4, "top": 6, "right": 900, "bottom": 30}});
    assert_eq!(
        kitted.api("POST", "/api/locations", Some(&outside)).status,
        400
    );
}

#[test]
fn each_choice_is_prepared_and_answered_with_its_readiness_kind() {
    let kitted = Kitted::new("play-w1-choices");
    let candidate = kitted.picture(19);
    let prepared = kitted.prepare("reality-checks", &json!({"candidate": candidate}));
    assert_eq!(prepared["ready"], true, "{prepared}");
    let judges = prepared["choices"]["judge"].as_array().unwrap().clone();
    let kind_of = |name: &str| {
        judges
            .iter()
            .find(|choice| choice["value"] == name)
            .map(|choice| (choice["ready"].clone(), choice["kind"].clone()))
    };
    assert_eq!(kind_of("cloud-judge"), Some((json!(true), json!("ready"))));
    assert_eq!(
        kind_of("keyless-judge"),
        Some((json!(false), json!("no_key")))
    );
    // Each need's bound: sixteen turns of a fifth of a cent.
    let need = &prepared["needs"][0];
    assert_eq!(
        (need["capability"].clone(), need["requests"].clone()),
        (json!("judge"), json!(16))
    );
    assert!(
        (prepared["bound"].as_f64().unwrap() - 0.032).abs() < 1e-9,
        "{prepared}"
    );
    // The judge that is not ready, picked: not ready, with what is missing, and no handle.
    let picked = kitted.prepare(
        "reality-checks",
        &json!({"candidate": candidate, "judge": "keyless-judge"}),
    );
    assert_eq!(picked["ready"], false, "{picked}");
    assert_eq!(picked["handle"], Json::Null);
    assert_eq!(picked["missing"][0]["kind"], "no_key", "{picked}");
}

#[test]
fn a_backend_is_ready_by_its_own_readiness_and_a_key_not_set_names_its_variable() {
    // A filler whose key the env file does not set (there is none), beside a segmenter
    // that needs no key.
    let config = kit_config("fill").replace(
        "[backends.fill]\nkind = \"playkit\"\nmodel = \"fill\"\n",
        "[backends.fill]\nkind = \"playkit\"\nmodel = \"fill\"\nkey_env = \"PLAYKIT_FILL_KEY\"\n",
    );
    assert!(config.contains("PLAYKIT_FILL_KEY"), "{config}");
    let kitted = Kitted::configured("play-own-readiness", &config);
    let arguments = json!({"image": kitted.picture(23), "prompt": "the hose"});
    let prepared = kitted.prepare("remove_prompt", &arguments);
    assert_eq!(prepared["ready"], false, "{prepared}");
    // The segmenter is ready: the filler's missing key is not its own.
    let segmenters = prepared["choices"]["backend"].as_array().unwrap();
    assert!(!segmenters.is_empty(), "{prepared}");
    for choice in segmenters {
        assert_ne!(choice["kind"], "no_key", "{choice}");
    }
    assert!(
        segmenters
            .iter()
            .any(|choice| choice["value"] == "default" && choice["ready"] == true),
        "{prepared}"
    );
    // What is missing says which variable to set, by its name, in plain words.
    let missing = &prepared["missing"][0];
    assert_eq!(missing["capability"], "fill", "{prepared}");
    assert_eq!(missing["kind"], "no_key", "{prepared}");
    assert_eq!(
        missing["why"], "the filler needs PLAYKIT_FILL_KEY in your env file",
        "{prepared}"
    );
    // The plan's fill is neither local nor free, and says why.
    let fill = prepared["needs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|need| need["capability"] == "fill")
        .unwrap();
    assert_eq!(fill["local"], false, "{fill}");
    assert_eq!(fill["ready"], false, "{fill}");
    assert_eq!(
        fill["why"], "the filler needs PLAYKIT_FILL_KEY in your env file",
        "{fill}"
    );
    let segment = prepared["needs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|need| need["capability"] == "segment")
        .unwrap();
    assert_eq!(segment["ready"], true, "{segment}");
}

#[test]
fn a_variable_is_named_only_when_it_has_the_shape_of_one() {
    use whizbang::backend_status::variable_shaped;
    for name in [
        "FAL_KEY",
        "GEMINI_API_KEY",
        "ANTHROPIC_API_KEY",
        "OPENAI_API_KEY",
        "PLAYKIT_TEST_KEY",
        "K2_V3",
    ] {
        assert!(variable_shaped(name), "{name}");
    }
    // A key pasted where its name belongs, or anything else, is not said back.
    for name in [
        // One word.
        "FALKEY",
        // A lower-case key with underscores.
        "sk_live_abc123_def456",
        "fal_key",
        // A long hex string, in either case.
        "9F86D081884C7D659A2FEAA0C55AD015A3BF4F1B2B0B822CD15D6C15B0F00A08",
        "9f86d081884c7d659a2feaa0c55ad015",
        // A word of thirteen characters.
        "ABCDEFGHIJKLM_KEY",
        // A leading digit, a double underscore, an underscore at either end.
        "2FAL_KEY",
        "FAL__KEY",
        "_FAL_KEY",
        "FAL_KEY_",
        "FAL-KEY",
        "",
    ] {
        assert!(!variable_shaped(name), "{name}");
    }
    // An AWS-style access key id: "AKIA" and sixteen upper-case letters and digits,
    // made here so that no file holds one.
    let id = format!("AKIA{}", "Q7".repeat(8));
    assert_eq!(id.len(), 20);
    assert!(!variable_shaped(&id), "{id}");
    // At most 64 characters in all, though each word is short enough.
    let long = ["ABCDEFGHIJKL"; 6].join("_");
    assert!(long.len() > 64);
    assert!(!variable_shaped(&long));
    assert!(variable_shaped(&["ABCDEFGHIJKL"; 5].join("_")));
}

#[test]
fn an_event_message_with_an_escape_sequence_is_answered_escaped() {
    let kitted = Kitted::new("play-w1-escaped");
    let prepared = kitted.prepare("shout", &json!({}));
    let started = kitted.begin("shout", &json!({}), &prepared["handle"], &fresh_id());
    assert_eq!(started.status, 201, "{}", started.text());
    // The run ends at once: its events come from its journal, escaped as a live one's.
    let ended = kitted.until_ended(started.json()["run"].as_str().unwrap());
    let said: Vec<&str> = ended["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|event| event["message"].as_str())
        .collect();
    let shout = said
        .iter()
        .find(|message| message.contains("cleared"))
        .unwrap();
    assert!(
        !shout.contains('\u{1b}') && !shout.contains('\u{7}'),
        "{shout:?}"
    );
    assert!(shout.contains("\\u{1b}"), "{shout:?}");
}

#[test]
fn hello_lists_the_kits_and_says_why_beside_an_empty_list_of_backends() {
    let scratch = Scratch::new("play-w1-hello");
    let config = scratch.path().join("whizbang.toml");
    std::fs::write(&config, "[backends.fill]\nkind = 3\n").unwrap();
    let over = Over::nothing().config(Some(&config));
    let registry = wiring::registry().unwrap();
    let play = Playground::new(registry.descriptors().cloned().collect(), Arc::new(over));
    let hello = whizbang_serve::play::Play::hello(&play);
    assert_eq!(hello["backends"], json!([]));
    assert!(
        hello["backends_error"]
            .as_str()
            .is_some_and(|why| !why.is_empty())
    );
    let kits: Vec<&str> = hello["kits"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|kit| kit["id"].as_str())
        .collect();
    assert_eq!(kits, ["rascal", "squint"]);
    let loop_ = hello["kits"][0]["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|task| task["name"] == "remove_checked")
        .unwrap();
    assert_eq!(
        (loop_["title"].clone(), loop_["uses"].clone()),
        (json!("Remove, checked"), json!("squint"))
    );
}

/// The raw request of the page, with one header more.
fn with_header(kitted: &Kitted, path: &str, header: &str) -> Answer {
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\n{header}\r\n\r\n",
        port = kitted.address.port(),
        token = kitted.token.hex(),
    );
    send(kitted.address, request.as_bytes())
}

#[test]
fn a_request_with_the_fixture_script_header_is_refused_400_by_the_real_host() {
    let kitted = Kitted::new("play-w1-script");
    let before = kitted.generation().number();
    for path in ["/api/hello", "/api/pictures", "/"] {
        let refused = with_header(&kitted, path, "x-fixture-script: hold");
        assert_eq!(refused.status, 400, "{path}: {}", refused.text());
    }
    // Nothing changed: the store, and the kit's script, which the real door never sets.
    assert_eq!(kitted.generation().number(), before);
    let prepared = kitted.prepare("shout", &words("free"));
    let started = kitted.begin("shout", &words("free"), &prepared["handle"], &fresh_id());
    let ended = kitted.until_ended(started.json()["run"].as_str().unwrap());
    assert_eq!(ended["outcome"], "completed");
}

/// Polls an ended run from this place.
fn poll_from(kitted: &Kitted, run: &str, after: u64) -> Json {
    kitted
        .api("GET", &format!("/api/runs/{run}?after={after}"), None)
        .json()
}

#[test]
fn an_ended_run_polled_from_a_place_is_answered_from_its_journal_from_there() {
    let kitted = Kitted::new("play-w1-after");
    let prepared = kitted.prepare("chatty", &json!({}));
    let started = kitted.begin("chatty", &json!({}), &prepared["handle"], &fresh_id());
    let run = started.json()["run"].as_str().unwrap().to_owned();
    let whole = kitted.until_ended(&run);
    let next = whole["next"].as_u64().unwrap();
    let later = poll_from(&kitted, &run, 10);
    let seqs: Vec<u64> = (later["events"].as_array().unwrap().iter())
        .filter_map(|event| event["seq"].as_u64())
        .collect();
    assert_eq!(seqs.first(), Some(&10), "{}", later["next"]);
    assert!(seqs.iter().all(|seq| *seq >= 10));
    assert_eq!(later["next"], next);
    let past = poll_from(&kitted, &run, next);
    assert_eq!(past["events"], json!([]));
    assert_eq!(
        (past["next"].clone(), past["skipped"].clone()),
        (json!(next), json!(0))
    );
}

#[test]
fn an_ended_run_past_its_kept_messages_is_polled_with_the_count_skipped() {
    let kitted = Kitted::new("play-w1-skipped");
    let prepared = kitted.prepare("chatty", &json!({}));
    let started = kitted.begin("chatty", &json!({}), &prepared["handle"], &fresh_id());
    let run = started.json()["run"].as_str().unwrap().to_owned();
    kitted.until_ended(&run);
    let kept = whizbang_core::contract::MESSAGES_KEPT_LIMIT as u64;
    // From near the end of what was kept: the last kept events, then the places of
    // those not kept counted as skipped.
    let tail = poll_from(&kitted, &run, kept - 2);
    let skipped = tail["skipped"].as_u64().unwrap();
    assert!(skipped >= 3, "{}", tail["skipped"]);
    let next = tail["next"].as_u64().unwrap();
    let shown = tail["events"].as_array().unwrap().len() as u64;
    assert_eq!(next - (kept - 2), shown + skipped, "{tail}");
}

#[test]
fn a_repeat_after_a_restart_is_answered_while_every_place_is_taken() {
    let kitted = Kitted::new("play-w1-full");
    let prepared = kitted.prepare("shout", &words("first"));
    let first = kitted.begin("shout", &words("first"), &prepared["handle"], "page-first");
    kitted.until_ended(first.json()["run"].as_str().unwrap());
    let kitted = kitted.restarted();
    let mut runs = Vec::new();
    for _ in 0..4 {
        let handle = kitted.prepare("waits", &json!({}))["handle"].clone();
        let started = kitted.begin("waits", &json!({}), &handle, &fresh_id());
        assert_eq!(started.status, 201, "{}", started.text());
        runs.push(started.json()["run"].as_str().unwrap().to_owned());
    }
    let repeat = kitted.begin("shout", &words("first"), &Json::Null, "page-first");
    assert_eq!(repeat.status, 200, "{}", repeat.text());
    assert_eq!(repeat.json()["run"], first.json()["run"]);
    // Under a config changed since, the same id is still its run's.
    let config = kitted.scratch.path().join("whizbang.toml");
    std::fs::write(&config, kit_config("other-fill")).unwrap();
    let again = kitted.begin("shout", &words("first"), &Json::Null, "page-first");
    assert_eq!(again.json()["run"], first.json()["run"], "{}", again.text());
    for run in &runs {
        kitted.api("POST", &format!("/api/runs/{run}/cancel"), None);
    }
}

#[test]
fn a_paid_run_says_what_the_ledger_settled_of_what_it_reserved() {
    let kitted = Kitted::new("play-w1-spend");
    let arguments = json!({"image": kitted.picture(30), "prompt": "the hose"});
    let prepared = kitted.prepare("remove_prompt", &arguments);
    // The config's name of the backend that answers: a lone table is named `default`.
    assert_eq!(prepared["needs"][1]["backend"], "default", "{prepared}");
    let bound = prepared["bound"].as_f64().unwrap();
    let started = kitted.begin(
        "remove_prompt",
        &arguments,
        &prepared["handle"],
        &fresh_id(),
    );
    let ended = kitted.until_ended(started.json()["run"].as_str().unwrap());
    assert_eq!(ended["outcome"], "completed", "{ended}");
    // One fill of four cents, settled, under a reservation of the preparation's bound.
    assert!(
        (ended["spent"].as_f64().unwrap() - 0.04).abs() < 1e-9,
        "{ended}"
    );
    assert!(
        (ended["reserved"].as_f64().unwrap() - bound).abs() < 1e-9,
        "{ended}"
    );
    // Hello, asked again once the run has ended (the top bar does), says what is spent
    // since the reset: the four cents, of the ten dollars; and what is left.
    let hello = kitted.api("GET", "/api/hello", None).json();
    let allowance = &hello["allowance"];
    assert_eq!(allowance["allowance"], 10.0, "{allowance}");
    assert!(
        (allowance["spent"].as_f64().unwrap() - 0.04).abs() < 1e-9,
        "{allowance}"
    );
    assert!(
        (allowance["left"].as_f64().unwrap() - 9.96).abs() < 1e-9,
        "{allowance}"
    );
    // Nothing is going, so nothing is held, and the count is exact.
    assert_eq!(allowance["held"], 0.0, "{allowance}");
    assert_eq!(allowance["exactly"], true, "{allowance}");
}

#[test]
fn hello_says_what_a_run_going_holds_apart_from_what_was_spent() {
    let kitted = Kitted::new("play-held");
    let arguments = json!({"image": kitted.picture(35), "prompt": "the hose"});
    let prepared = kitted.prepare("remove_prompt", &arguments);
    let bound = prepared["bound"].as_f64().unwrap();
    kitted.kit().set("hold").unwrap();
    let started = kitted.begin(
        "remove_prompt",
        &arguments,
        &prepared["handle"],
        &fresh_id(),
    );
    assert_eq!(started.status, 201, "{}", started.text());
    let run = started.json()["run"].as_str().unwrap().to_owned();
    // While the run is held, its reservation is held, and nothing is spent yet.
    let limit = Instant::now() + Duration::from_secs(20);
    let allowance = loop {
        let hello = kitted.api("GET", "/api/hello", None).json();
        if hello["allowance"]["held"].as_f64().unwrap_or(0.0) > 0.0 {
            break hello["allowance"].clone();
        }
        assert!(Instant::now() < limit, "nothing was ever held: {hello}");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(
        (allowance["held"].as_f64().unwrap() - bound).abs() < 1e-9,
        "{allowance}"
    );
    assert_eq!(allowance["spent"], 0.0, "{allowance}");
    kitted.kit().set("release").unwrap();
    kitted.until_ended(&run);
}

#[test]
fn the_artifact_route_shows_each_type_it_can_and_refuses_the_rest() {
    let kitted = Kitted::new("play-w1-artifacts");
    let picture = kitted.picture(31);
    for id in [
        "not-an-id",
        "sha256:0000000000000000000000000000000000000000000000000000000000000000",
    ] {
        assert_eq!(
            kitted
                .api("GET", &format!("/api/artifacts/{id}"), None)
                .status,
            404,
            "{id}"
        );
    }
    // A mask, made by a task that needs no model, shown as a grey picture.
    let arguments = json!({"image": picture, "box": "4 4 20 20"});
    let handle = kitted.prepare("mask_rect", &arguments)["handle"].clone();
    let started = kitted.begin("mask_rect", &arguments, &handle, &fresh_id());
    let ended = kitted.until_ended(started.json()["run"].as_str().unwrap());
    let mask = (ended["outputs"].as_array().unwrap().iter())
        .find(|output| output["type"] == "mask")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let shown = kitted.api("GET", &format!("/api/artifacts/{mask}"), None);
    assert_eq!(shown.status, 200);
    // The PNG's colour type, in its header: 0, grey.
    assert_eq!(shown.body[25], 0, "a grey picture");
    // A series, which has no picture to show.
    let inputs = json!({"mask": mask, "image": picture});
    let handle = kitted.prepare("inspect", &inputs)["handle"].clone();
    let started = kitted.begin("inspect", &inputs, &handle, &fresh_id());
    let ended = kitted.until_ended(started.json()["run"].as_str().unwrap());
    let series = (ended["outputs"].as_array().unwrap().iter())
        .find(|output| output["type"] == "series")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        kitted
            .api("GET", &format!("/api/artifacts/{series}"), None)
            .status,
        415
    );
    // Thumbnails: an edge from 8 to 2048, and the picture no larger than it.
    for edge in [7, 2049] {
        let path = format!("/api/artifacts/{picture}/thumb?edge={edge}");
        assert_eq!(kitted.api("GET", &path, None).status, 400, "{edge}");
    }
    let thumb = kitted.api(
        "GET",
        &format!("/api/artifacts/{picture}/thumb?edge=16"),
        None,
    );
    assert_eq!(thumb.status, 200);
    let size = image::load_from_memory(&thumb.body).unwrap();
    assert!(size.width() <= 16 && size.height() <= 16);
    // Asked again: the same bytes, kept.
    let again = kitted.api(
        "GET",
        &format!("/api/artifacts/{picture}/thumb?edge=16"),
        None,
    );
    assert_eq!(again.body, thumb.body);
}

#[test]
fn the_pictures_are_paged_newest_first_and_a_picture_sent_again_keeps_its_newest_name() {
    let kitted = Kitted::new("play-w1-paging");
    let ids: Vec<String> = (40..43).map(|shade| kitted.picture(shade)).collect();
    let first = kitted.api("GET", "/api/pictures?limit=2", None).json();
    let listed: Vec<&str> = (first["pictures"].as_array().unwrap().iter())
        .filter_map(|p| p["id"].as_str())
        .collect();
    assert_eq!(listed, [ids[2].as_str(), ids[1].as_str()]);
    let rest = kitted
        .api(
            "GET",
            &format!("/api/pictures?limit=2&after={}", ids[1]),
            None,
        )
        .json();
    assert_eq!(rest["pictures"].as_array().unwrap().len(), 1);
    assert_eq!(rest["pictures"][0]["id"], ids[0].as_str());
    let unknown = format!("/api/pictures?after=sha256:{}", "1".repeat(64));
    assert_eq!(kitted.api("GET", &unknown, None).status, 400);
    // The oldest sent again under another name: one entry, the newest name, first.
    let again = kitted.upload(&png(64, 48, 40), "renamed.png");
    assert_eq!(again.json()["id"], ids[0].as_str());
    let all = kitted.api("GET", "/api/pictures", None).json();
    assert_eq!(all["pictures"].as_array().unwrap().len(), 3);
    assert_eq!(all["pictures"][0]["name"], "renamed.png");
}

#[test]
fn a_picture_is_looked_up_by_its_identity_and_one_not_kept_answers_none() {
    let kitted = Kitted::new("play-gaps-picture");
    let ids: Vec<String> = (44..47).map(|shade| kitted.picture(shade)).collect();
    // The oldest, past a first page of one: found by its identity, with its name.
    let found = kitted
        .api("GET", &format!("/api/pictures?id={}", ids[0]), None)
        .json();
    let listed = found["pictures"].as_array().unwrap();
    assert_eq!(listed.len(), 1, "{found}");
    assert_eq!(listed[0]["id"], ids[0].as_str());
    assert!(listed[0]["name"].is_string(), "{found}");
    // An identity the playground kept no picture under: none.
    let unknown = format!("/api/pictures?id=sha256:{}", "1".repeat(64));
    assert_eq!(
        kitted.api("GET", &unknown, None).json()["pictures"],
        json!([])
    );
    // Not an identity: refused.
    assert_eq!(
        kitted.api("GET", "/api/pictures?id=porch", None).status,
        400
    );
}

#[test]
fn a_line_is_drawn_and_a_path_or_polygon_past_its_points_is_refused() {
    let kitted = Kitted::new("play-w1-lines");
    let picture = kitted.picture(50);
    let line = json!({"picture": picture,
                      "shape": {"kind": "line", "points": [[2, 2], [40, 30]], "width": 4}});
    let made = kitted.api("POST", "/api/locations", Some(&line));
    assert_eq!(made.status, 201, "{}", made.text());
    let id = made.json()["id"].as_str().unwrap().to_owned();
    let shown = kitted
        .api("GET", &format!("/api/artifacts/{id}"), None)
        .json();
    assert_eq!(shown["kind"], "line");
    let long: Vec<Json> = (0..513).map(|at| json!([at % 60, at % 40])).collect();
    for kind in ["stroke", "line"] {
        let shape = json!({"picture": picture,
                           "shape": {"kind": kind, "points": long, "radius": 3, "width": 3}});
        let mut shape = shape;
        if kind == "stroke" {
            shape["shape"].as_object_mut().unwrap().remove("width");
        } else {
            shape["shape"].as_object_mut().unwrap().remove("radius");
        }
        assert_eq!(
            kitted.api("POST", "/api/locations", Some(&shape)).status,
            400,
            "{kind}"
        );
    }
    let most: Vec<Json> = (0..512).map(|at| json!([at % 60, at % 40])).collect();
    let stroke =
        json!({"picture": picture, "shape": {"kind": "stroke", "points": most, "radius": 3}});
    assert_eq!(
        kitted.api("POST", "/api/locations", Some(&stroke)).status,
        201
    );
}

#[test]
fn two_pictures_sent_at_once_are_both_kept() {
    let kitted = Arc::new(Kitted::new("play-w1-two"));
    let sent: Vec<_> = (0..2u8)
        .map(|at| {
            let kitted = kitted.clone();
            whizbang_testkit::threads::spawn(move || {
                kitted.upload(&png(64, 48, 60 + at), "both.png")
            })
        })
        .collect();
    let ids: Vec<String> = sent
        .into_iter()
        .map(|handle| {
            let answer = handle.join().unwrap();
            assert_eq!(answer.status, 201, "{}", answer.text());
            answer.json()["id"].as_str().unwrap().to_owned()
        })
        .collect();
    assert_ne!(ids[0], ids[1]);
    let listed = kitted.api("GET", "/api/pictures", None).json();
    assert_eq!(listed["pictures"].as_array().unwrap().len(), 2);
}

#[test]
fn a_thumbnail_asked_for_again_is_the_one_kept_and_not_made_twice() {
    let kitted = Kitted::new("play-w1-kept-thumb");
    let picture = kitted.picture(70);
    let path = format!("/api/artifacts/{picture}/thumb?edge=24");
    let first = kitted.api("GET", &path, None);
    assert_eq!(first.status, 200);
    assert_eq!(kitted.door.thumbnails_made(), 1);
    let again = kitted.api("GET", &path, None);
    assert_eq!(again.body, first.body);
    assert_eq!(kitted.door.thumbnails_made(), 1, "taken from those kept");
    // Another edge is another thumbnail.
    let other = kitted.api(
        "GET",
        &format!("/api/artifacts/{picture}/thumb?edge=12"),
        None,
    );
    assert_eq!(other.status, 200);
    assert_eq!(kitted.door.thumbnails_made(), 2);
}

// History, its figures, reports and the tool records (plan m5, task W3).

impl Kitted {
    /// Runs a task to its end, as the page runs it: prepared, started, polled.
    fn ran(&self, action: &str, arguments: &Json) -> String {
        let prepared = self.prepare(action, arguments);
        assert_eq!(prepared["ready"], true, "{prepared}");
        let started = self.begin(action, arguments, &prepared["handle"], &fresh_id());
        assert_eq!(started.status, 201, "{}", started.text());
        let run = started.json()["run"].as_str().unwrap().to_owned();
        self.until_ended(&run);
        run
    }

    fn listed(&self, query: &str) -> Vec<Json> {
        let answer = self.api("GET", &format!("/api/runs?{query}"), None);
        assert_eq!(answer.status, 200, "{query}: {}", answer.text());
        answer.json()["runs"].as_array().unwrap().clone()
    }

    fn report(&self, run: &str) -> String {
        let answer = self.api("GET", &format!("/api/runs/{run}/report"), None);
        assert_eq!(answer.status, 200, "{}", answer.text());
        answer.text()
    }
}

fn runs_of(rows: &[Json]) -> Vec<&str> {
    rows.iter()
        .map(|row| row["run"].as_str().unwrap())
        .collect()
}

#[test]
fn history_filters_by_kit_task_result_and_words() {
    let kitted = Kitted::new("play-w3-history");
    let porch = kitted.upload(&png(64, 48, 90), "Back%20porch.png");
    let porch = porch.json()["id"].as_str().unwrap().to_owned();
    let removed = kitted.ran(
        "remove_prompt",
        &json!({"image": porch, "prompt": "the garden HOSE"}),
    );
    let passed = kitted.ran("reality-checks", &json!({"candidate": kitted.picture(91)}));
    kitted.kit.set("fail").unwrap();
    let failed = kitted.ran("reality-checks", &json!({"candidate": kitted.picture(92)}));
    kitted.kit.set("reset").unwrap();
    // Newest first, the runs nothing nested only, each with what the page shows.
    let all = kitted.listed("");
    assert_eq!(
        runs_of(&all),
        [failed.as_str(), passed.as_str(), removed.as_str()]
    );
    let row = &all[2];
    assert_eq!(row["kit"], "rascal");
    assert_eq!(row["title"], "Remove what you name");
    assert_eq!(row["result"], "done");
    assert_eq!(row["words"], json!(["the garden HOSE"]));
    assert_eq!(row["pictures"], json!(["Back porch.png"]));
    assert_eq!(row["thumb"], json!(porch));
    assert!(row["cost"].as_f64().unwrap() > 0.0, "{row}");
    assert!(row["time_ms"].is_u64() && row["at_ms"].is_u64(), "{row}");
    assert_eq!(all[0]["result"], "fail");
    // Each filter narrows the list.
    assert_eq!(
        runs_of(&kitted.listed("kit=squint")),
        [failed.as_str(), passed.as_str()]
    );
    assert_eq!(runs_of(&kitted.listed("result=fail")), [failed.as_str()]);
    assert_eq!(
        runs_of(&kitted.listed("task=remove_prompt")),
        [removed.as_str()]
    );
    assert_eq!(runs_of(&kitted.listed("words=hose")), [removed.as_str()]);
    assert_eq!(
        runs_of(&kitted.listed("words=BACK%20PORCH")),
        [removed.as_str()]
    );
    assert_eq!(
        runs_of(&kitted.listed("kit=rascal&result=pass")),
        Vec::<&str>::new()
    );
    // By pages.
    let first = kitted.api("GET", "/api/runs?limit=2", None).json();
    assert_eq!(
        runs_of(first["runs"].as_array().unwrap()),
        [failed.as_str(), passed.as_str()]
    );
    assert_eq!(first["more"], true);
    let next = kitted.listed(&format!("limit=2&before={passed}"));
    assert_eq!(runs_of(&next), [removed.as_str()]);
    // What it does not take.
    for bad in [
        "result=maybe",
        "limit=0",
        "before=nothing",
        "since=yesterday",
    ] {
        let answer = kitted.api("GET", &format!("/api/runs?{bad}"), None);
        assert_eq!(answer.status, 400, "{bad}: {}", answer.text());
    }
    assert_eq!(kitted.api("GET", "/api/runs?colour=red", None).status, 400);
    // W1's list of the runs still going is the same route.
    let open = kitted.api("GET", "/api/runs?open=1", None).json();
    assert_eq!(open["runs"], json!([]));
    // History survives a restart.
    let kitted = kitted.restarted();
    assert_eq!(
        runs_of(&kitted.listed("")),
        [failed.as_str(), passed.as_str(), removed.as_str()]
    );
}

#[test]
fn history_lists_the_runs_that_made_an_output_of_a_type_with_their_outputs() {
    let kitted = Kitted::new("play-gaps-outputs");
    let removed = kitted.ran(
        "remove_prompt",
        &json!({"image": kitted.picture(95), "prompt": "the rake"}),
    );
    kitted.ran("reality-checks", &json!({"candidate": kitted.picture(96)}));
    // Each row names its outputs, by port, type and identity.
    let all = kitted.listed("");
    let outputs = all[1]["outputs"].as_array().unwrap();
    let mask = outputs
        .iter()
        .find(|output| output["type"] == "mask")
        .unwrap_or_else(|| panic!("a removal makes a mask: {}", all[1]));
    assert!(
        mask["id"].as_str().unwrap().starts_with("sha256:"),
        "{mask}"
    );
    assert!(mask["port"].is_string(), "{mask}");
    // `output=` lists only the runs that made one of that type, by pages as before.
    assert_eq!(runs_of(&kitted.listed("output=mask")), [removed.as_str()]);
    assert_eq!(
        runs_of(&kitted.listed("output=band_profile")),
        Vec::<&str>::new()
    );
    assert_eq!(
        runs_of(&kitted.listed(&format!("output=mask&before={removed}"))),
        Vec::<&str>::new()
    );
    for bad in ["output=Mask!", "output="] {
        let answer = kitted.api("GET", &format!("/api/runs?{bad}"), None);
        assert_eq!(answer.status, 400, "{bad}: {}", answer.text());
    }
    // From the store, so a restart keeps them.
    let kitted = kitted.restarted();
    assert_eq!(runs_of(&kitted.listed("output=mask")), [removed.as_str()]);
}

#[test]
fn history_of_an_output_type_is_paged_after_the_last_run_shown() {
    let kitted = Kitted::new("play-gaps-output-pages");
    let removed: Vec<String> = (97..100)
        .map(|shade| {
            kitted.ran(
                "remove_prompt",
                &json!({"image": kitted.picture(shade), "prompt": "the rake"}),
            )
        })
        .collect();
    kitted.ran("reality-checks", &json!({"candidate": kitted.picture(100)}));
    let mut before: Option<String> = None;
    let mut seen = Vec::new();
    let mut mores = Vec::new();
    for _ in 0..3 {
        let path = match &before {
            Some(run) => format!("/api/runs?output=mask&limit=1&before={run}"),
            None => "/api/runs?output=mask&limit=1".to_owned(),
        };
        let page = kitted.api("GET", &path, None).json();
        let runs = page["runs"].as_array().unwrap();
        assert_eq!(runs.len(), 1, "{page}");
        let run = runs[0]["run"].as_str().unwrap().to_owned();
        seen.push(run.clone());
        mores.push(page["more"].as_bool().unwrap());
        before = Some(run);
    }
    assert_eq!(mores, [true, true, false]);
    let newest_first: Vec<String> = removed.into_iter().rev().collect();
    assert_eq!(seen, newest_first);
}

#[test]
fn history_and_the_figures_read_the_store_again_only_when_it_has_published_since() {
    let kitted = Kitted::new("play-w3-shelved");
    kitted.ran("shout", &json!({}));
    assert_eq!(kitted.listed("").len(), 1);
    let read = kitted.door.records_read();
    assert!(read >= 1);
    // The same generation: another page, another filter and the figures read nothing new.
    assert_eq!(kitted.listed("words=nothing").len(), 0);
    assert_eq!(kitted.api("GET", "/api/stats", None).status, 200);
    assert_eq!(kitted.door.records_read(), read);
    // A run published since: read again.
    kitted.ran("shout", &json!({}));
    assert_eq!(kitted.listed("").len(), 2);
    assert!(kitted.door.records_read() > read);
}

/// A store holding a loop, a decision answered from the cache, unsure verdicts, a
/// cancelled run with spend left unresolved, and runs of both kits.
fn figured() -> Kitted {
    let kitted = Kitted::cached("play-w3-figures");
    let candidate = kitted.picture(100);
    kitted.ran("reality-checks", &json!({"candidate": candidate}));
    kitted.ran("reality-checks", &json!({"candidate": candidate}));
    kitted.kit.set("unsure").unwrap();
    kitted.ran("reality-checks", &json!({"candidate": kitted.picture(101)}));
    kitted.kit.set("reset").unwrap();
    kitted.ran(
        "remove_checked",
        &json!({"image": kitted.picture(102), "prompt": "the hose"}),
    );
    // A fill held, then cancelled: its spend is left unresolved.
    kitted.kit.set("hold").unwrap();
    let arguments = json!({"image": kitted.picture(103), "prompt": "the hose"});
    let prepared = kitted.prepare("remove_prompt", &arguments);
    let before = kitted.kit.requests();
    let started = kitted.begin(
        "remove_prompt",
        &arguments,
        &prepared["handle"],
        &fresh_id(),
    );
    let run = started.json()["run"].as_str().unwrap().to_owned();
    let arrived = |count: u64| {
        for _ in 0..4000 {
            if kitted.kit.requests() >= count {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("the request never arrived");
    };
    // The segmenter's request, held; then the fill's.
    arrived(before + 1);
    kitted.kit.set("release,hold").unwrap();
    arrived(before + 2);
    assert_eq!(
        kitted
            .api("POST", &format!("/api/runs/{run}/cancel"), None)
            .status,
        202
    );
    kitted.until_ended(&run);
    kitted.kit.set("reset").unwrap();
    kitted
}

#[test]
fn stats_answers_what_runs_stats_answers_for_the_same_filter() {
    let kitted = figured();
    let root = kitted.root();
    for (query, flags) in [
        ("", vec![]),
        (
            "kit=squint&result=unsure",
            vec!["--kit", "squint", "--result", "unsure"],
        ),
        (
            "task=remove_checked&words=hose",
            vec!["--action", "remove_checked", "--words", "hose"],
        ),
    ] {
        let answer = kitted.api("GET", &format!("/api/stats?{query}"), None);
        assert_eq!(answer.status, 200, "{query}: {}", answer.text());
        let figures = answer.json();
        let mut args = vec!["runs", "stats", "--unsure-by", "rule"];
        args.extend(flags);
        let printed = crate::wired::whizbang(&args, &root);
        assert_eq!(printed.code, Some(0), "{}", printed.err);
        let text = figures["text"].as_str().unwrap();
        assert!(
            printed.out.starts_with(text),
            "{query}:\n{text}\n---\n{}",
            printed.out
        );
        if query.is_empty() {
            assert!(text.contains("cache hits: 1\n"), "{text}");
            assert!(text.contains("loops: 1 runs"), "{text}");
            assert!(
                figures["unresolved_micros"].as_u64().unwrap() > 0,
                "{figures}"
            );
            assert!(
                figures["verdicts"]["unsure"].as_u64().unwrap() >= 1,
                "{figures}"
            );
            assert!(printed.out.contains("unsure by rule: "), "{}", printed.out);
            assert_eq!(
                figures["unsure_by_rule"]["verdicts"].as_u64(),
                figures["verdicts"]["unsure"].as_u64()
            );
        }
    }
    assert_eq!(kitted.api("GET", "/api/stats?until=soon", None).status, 400);
}

#[test]
fn the_embedded_records_are_the_reference_folder() {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/reference/tools");
    let mut on_disk = Vec::new();
    let mut next = vec![folder.clone()];
    while let Some(dir) = next.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                next.push(path);
            } else {
                let name = path.strip_prefix(&folder).unwrap();
                on_disk.push(name.to_str().unwrap().replace('\\', "/"));
            }
        }
    }
    on_disk.sort();
    let embedded: Vec<String> = whizbang_serve::play::records::paths()
        .map(str::to_owned)
        .collect();
    let missing: Vec<&String> = on_disk.iter().filter(|p| !embedded.contains(p)).collect();
    let extra: Vec<&String> = embedded.iter().filter(|p| !on_disk.contains(p)).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "crates/whizbang-serve/src/play/records.rs lists the folder: add {missing:?}, remove {extra:?}"
    );
    for path in &embedded {
        let (_, bytes) = whizbang_serve::play::records::find(path).unwrap();
        assert_eq!(bytes, std::fs::read(folder.join(path)).unwrap(), "{path}");
    }
    // Served by name, with its images, behind the guard.
    let kitted = Kitted::new("play-w3-records");
    let page = kitted.api("GET", "/api/records/remove_prompt", None);
    assert_eq!(page.status, 200);
    assert!(page.text().starts_with("# remove_prompt"));
    let names = kitted.api("GET", "/api/records", None).json();
    assert!(
        names["records"]
            .as_array()
            .unwrap()
            .contains(&json!("view"))
    );
    let image = kitted.api("GET", "/api/records/view/scene.png", None);
    assert_eq!((image.status, &image.body[1..4]), (200, &b"PNG"[..]));
    for missing in [
        "/api/records/nothing",
        "/api/records/view/..%2Fview.md",
        "/api/records/view/none.png",
    ] {
        assert_eq!(kitted.api("GET", missing, None).status, 404, "{missing}");
    }
    let unguarded = api(
        kitted.address,
        "0".repeat(64).as_str(),
        "GET",
        "/api/records/view",
        None,
    );
    assert_eq!(unguarded.status, 401);
}

mod report {
    //! `play::report::…`: a run's report (plan m5, task W3, items 3 to 5).
    use super::*;
    use whizbang_core::capability::written::{Keep, for_display, write_exchange};
    use whizbang_core::capability::{
        Controls, Fault, FaultKind, JudgeRequest, Message, Opaque, Part, Picture, Role, StopReason,
        Turn,
    };
    use whizbang_core::identity::CanonicalPixels;

    /// No way for a document to run or load anything.
    fn inert(html: &str) {
        let lower = html.to_lowercase();
        for said in [
            "<script",
            "javascript:",
            "<iframe",
            "<link",
            "<form",
            "<object",
            "<embed",
            "<base",
            "http://",
            "https://",
            "src=\"/",
            "url(",
        ] {
            assert!(!lower.contains(said), "the report holds {said:?}");
        }
        // No tag carries an event handler.
        for tag in lower.split('<').skip(1) {
            let tag = tag.split('>').next().unwrap_or_default();
            let handler = tag
                .split_whitespace()
                .skip(1)
                .any(|word| word.starts_with("on"));
            assert!(!handler, "a tag with a handler: <{tag}>");
        }
        assert!(html.contains(
            "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; img-src data:; style-src 'unsafe-inline'\">"
        ));
    }

    #[test]
    fn a_squint_report_holds_its_verdict_findings_places_and_turns_and_no_script() {
        let kitted = Kitted::new("play-w3-report-squint");
        kitted.kit.set("fail,placed").unwrap();
        let run = kitted.ran("reality-checks", &json!({"candidate": kitted.picture(110)}));
        let html = kitted.report(&run);
        inert(&html);
        for said in [
            "Reality check",
            "<span class=\"verdict verdict--fail\">FAIL</span>",
            "<h2>Reasons</h2>",
            "<h2>Checklist</h2>",
            "violation",
            "The judge's turns",
            "turn 1",
            "calls submit_verdict",
            "<h2>Cost</h2>",
            "$0.0020",
        ] {
            assert!(html.contains(said), "{said}:\n{html}");
        }
        // The place the judge found, drawn on the candidate.
        assert!(html.contains("1 place the judge found"), "{html}");
        assert!(
            html.contains("<rect x=\"0.25\" y=\"0.25\" width=\"0.5\" height=\"0.5\""),
            "{html}"
        );
        // The same after a restart, and as a download named for its task and run.
        let kitted = kitted.restarted();
        let again = kitted.api("GET", &format!("/api/runs/{run}/report"), None);
        assert_eq!(again.text(), html);
    }

    #[test]
    fn every_text_from_a_model_or_a_person_is_escaped() {
        let kitted = Kitted::new("play-w3-report-escaped");
        kitted.kit.set("markup").unwrap();
        let brief = "<b onmouseover=\"x()\">a kitchen</b> & <script>y()</script>";
        let run = kitted.ran(
            "brief-compliance",
            &json!({"candidate": kitted.picture(111), "brief": brief}),
        );
        let html = kitted.report(&run);
        inert(&html);
        assert!(html.contains("&lt;b onmouseover="), "{html}");
        assert!(
            html.contains("a kitchen&lt;/b&gt; &amp; &lt;script&gt;y()&lt;/script&gt;"),
            "{html}"
        );
        assert!(
            html.contains("&lt;img src=x onerror=&quot;document.title=&#39;ran&#39;&quot;&gt;"),
            "{html}"
        );
        assert!(!html.contains("<b onmouseover") && !html.contains("<img src=x"));
    }

    #[test]
    fn the_document_opens_alone_with_its_pictures_inline() {
        let kitted = Kitted::new("play-w3-report-alone");
        let run = kitted.ran(
            "remove_prompt",
            &json!({"image": kitted.picture(112), "prompt": "the hose"}),
        );
        let answer = kitted.api("GET", &format!("/api/runs/{run}/report"), None);
        let html = answer.text();
        inert(&html);
        for said in [
            "Remove what you name",
            "<h2>Masks over the picture</h2>",
            "<h2>Steps</h2>",
            "segment",
            "fill",
            "given: image",
            "made: image",
        ] {
            assert!(html.contains(said), "{said}:\n{html}");
        }
        // Every picture inline, and none named by an address.
        let inline = html.matches("src=\"data:image/png;base64,").count();
        assert_eq!(inline, html.matches("src=\"").count());
        assert!(inline >= 3, "{inline}");
        // The archive: the record, the report and each output.
        let archive = kitted.api("GET", &format!("/api/runs/{run}/export"), None);
        assert_eq!(archive.status, 200);
        let names: Vec<String> = archive
            .body
            .chunks(512)
            .filter(|block| block[257..262] == *b"ustar")
            .map(|block| {
                String::from_utf8_lossy(&block[..100])
                    .trim_end_matches('\0')
                    .to_owned()
            })
            .collect();
        for name in [
            "record.json",
            "report.html",
            "outputs/image.image",
            "outputs/image.png",
            "outputs/mask.mask",
            "outputs/mask.png",
        ] {
            assert!(names.contains(&name.to_owned()), "{name}: {names:?}");
        }
        assert_eq!(
            kitted.api("GET", "/api/runs/r-none/report", None).status,
            404
        );
    }

    #[test]
    fn a_reports_escaping_keeps_tabs_ends_lines_once_and_shows_a_change_of_direction() {
        let said = whizbang::report::escape("a\tb\r\nc\u{202e}d\u{2066}e\u{7}<");
        assert_eq!(said, "a\tb\nc\\u{202e}d\\u{2066}e\\u{7}&lt;");
    }

    fn picture(shade: u8) -> Picture {
        let rgba = (0..6u8).flat_map(|at| [shade, at, 0, 255]).collect();
        Picture::new(CanonicalPixels::from_rgba(3, 2, rgba).unwrap()).unwrap()
    }

    #[test]
    fn a_withheld_part_a_missing_picture_and_a_failed_turn_each_show_as_such() {
        let opaque = Part::Opaque(Opaque::new("thinking", "{\"signature\":\"zz9\"}").unwrap());
        let said = vec![Part::Text("<b>I looked</b>".to_owned()), opaque];
        let turn = Turn::new(said, StopReason::EndTurn, None, None).unwrap();
        let messages = vec![
            Message::new(
                Role::User,
                vec![Part::Text("Look.".to_owned()), Part::Image(picture(3))],
            )
            .unwrap(),
        ];
        let request = JudgeRequest::new(
            "You judge.",
            messages,
            Vec::new(),
            Controls::new(64).unwrap(),
        )
        .unwrap();
        let none = |_| false;
        let written = write_exchange(0, &request, &Ok(turn), Keep::Digests);
        let html = whizbang::report::answer_html(&for_display(&written, &none).unwrap());
        assert!(html.contains("&lt;b&gt;I looked&lt;/b&gt;"), "{html}");
        assert!(
            html.contains("[part 2 withheld: 19 bytes of thinking, kept as its digest]"),
            "{html}"
        );
        assert!(html.contains("1 pictures not in the store"), "{html}");
        let fault = Fault::new(FaultKind::ALL[0], "the judge went away");
        let failed = write_exchange(1, &request, &Err(fault), Keep::Digests);
        let html = whizbang::report::answer_html(&for_display(&failed, &none).unwrap());
        assert!(
            html.contains("the turn failed: ") && html.contains("the judge went away"),
            "{html}"
        );
    }
}

mod stats {
    //! `play::stats::…`: the figures, counted for the command and the page alike.
    use super::*;

    #[test]
    fn cache_hits_are_counted_from_the_records() {
        let kitted = Kitted::cached("play-w3-cache-hits");
        let candidate = kitted.picture(120);
        let first = kitted.ran("reality-checks", &json!({"candidate": candidate}));
        let printed = crate::wired::whizbang(&["runs", "stats"], &kitted.root());
        assert!(printed.out.contains("cache hits: 0\n"), "{}", printed.out);
        let second = kitted.ran("reality-checks", &json!({"candidate": candidate}));
        assert_ne!(first, second);
        let printed = crate::wired::whizbang(&["runs", "stats"], &kitted.root());
        assert!(printed.out.contains("cache hits: 1\n"), "{}", printed.out);
        let figures = kitted.api("GET", "/api/stats", None).json();
        assert_eq!(figures["cache_hits"], 1, "{figures}");
    }
}

/// The writer "Export run" had before it moved to the core's (plan m7, task B1, item 2):
/// one file of a tar archive, its header and its bytes, padded to a block, its name cut
/// at 100 bytes. Kept here, word for word, to hold the move to the same bytes.
fn tar_file_before_the_move(out: &mut Vec<u8>, name: &str, bytes: &[u8], mtime: u64) {
    let mut header = [0u8; 512];
    let name = name.as_bytes();
    header[..name.len().min(100)].copy_from_slice(&name[..name.len().min(100)]);
    let octal = |field: &mut [u8], value: u64| {
        let text = format!("{value:0width$o}\0", width = field.len() - 1);
        field.copy_from_slice(text.as_bytes());
    };
    octal(&mut header[100..108], 0o644);
    octal(&mut header[108..116], 0);
    octal(&mut header[116..124], 0);
    octal(&mut header[124..136], bytes.len() as u64);
    octal(&mut header[136..148], mtime);
    header[148..156].copy_from_slice(b"        ");
    header[156] = b'0';
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    let sum: u32 = header.iter().map(|b| u32::from(*b)).sum();
    let text = format!("{sum:06o}\0 ");
    header[148..156].copy_from_slice(text.as_bytes());
    out.extend_from_slice(&header);
    out.extend_from_slice(bytes);
    let pad = (512 - bytes.len() % 512) % 512;
    out.extend(std::iter::repeat_n(0u8, pad));
}

#[test]
fn export_run_is_the_same_bytes_after_the_writer_moved() {
    use whizbang_core::bundle::tar;
    let chain = crate::bundle::chain("play-export-run");
    let run = RunId::parse(&chain.removal).unwrap();
    let archive = whizbang::report::archive(&chain.world.root, &run, None).unwrap();
    let mut reader = tar::Reader::new(std::io::Cursor::new(&archive), 1 << 30);
    let mut members = Vec::new();
    while let Some(head) = reader.next_head().unwrap() {
        let bytes = reader.bytes(&head).unwrap();
        members.push((head.name, bytes));
    }
    let names: Vec<&str> = members.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        names,
        [
            "record.json",
            "report.html",
            "outputs/image.image",
            "outputs/image.png",
            "outputs/mask.mask",
            "outputs/mask.png"
        ]
    );
    let record = whizbang_core::contract::RunRecord::from_bytes(&members[0].1).unwrap();
    let mtime = record.time().end_ms / 1000;
    assert!(mtime > 0);
    let mut before = Vec::new();
    for (name, bytes) in &members {
        tar_file_before_the_move(&mut before, name, bytes, mtime);
    }
    before.extend(std::iter::repeat_n(0u8, 1024));
    assert_eq!(archive, before);
}
