//! `serve_http::…`: `whizbang serve --http`, the engine API (plan m6, task H1; ADR 0045).
//! What it does is shown against the engine fixture (`examples/engine_fixture.rs`): the
//! engine's production code and the binary's wiring and door, over the test kit's fake
//! segmenter and filler, in this process. Its start, its flags, its token and its signals
//! are shown on the shipped binary itself. Requests are bytes on a loopback socket.

#[path = "../../examples/engine_fixture.rs"]
#[allow(dead_code)]
mod engine_fixture;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde_json::{Value as Json, json};
use whizbang::wiring::{self, Make};
use whizbang_serve::engine::{Settings, Stopped};
use whizbang_testkit::scratch::Scratch;

use engine_fixture::{Fixture, TOKEN};

/// An answer: its status, its head, its body.
struct Answer {
    status: u16,
    head: String,
    body: Vec<u8>,
}

impl Answer {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    fn json(&self) -> Json {
        serde_json::from_slice(&self.body).unwrap_or(Json::Null)
    }

    fn header(&self, name: &str) -> Option<String> {
        self.head.lines().skip(1).find_map(|line| {
            let (named, value) = line.split_once(':')?;
            named
                .eq_ignore_ascii_case(name)
                .then(|| value.trim().to_owned())
        })
    }

    /// The JSON-RPC messages of an event stream, in order.
    fn events(&self) -> Vec<Json> {
        self.text()
            .split("\n\n")
            .filter_map(|event| {
                let data = event.lines().find_map(|line| line.strip_prefix("data: "))?;
                serde_json::from_str(data).ok()
            })
            .collect()
    }

    /// The tool's result: of a stream, its last message; of a body, the body.
    fn result(&self) -> Json {
        let message = match self.header("content-type").as_deref() {
            Some("text/event-stream") => self.events().pop().unwrap_or(Json::Null),
            _ => self.json(),
        };
        message["result"].clone()
    }
}

fn parse(bytes: &[u8]) -> Answer {
    let end = bytes
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .unwrap_or_else(|| panic!("no answer: {:?}", String::from_utf8_lossy(bytes)));
    let head = String::from_utf8_lossy(&bytes[..end]).into_owned();
    Answer {
        status: head.split(' ').nth(1).unwrap().parse().unwrap(),
        head,
        body: bytes[end + 4..].to_vec(),
    }
}

/// A client of one engine: its address and its token.
struct Client {
    address: SocketAddr,
    token: String,
}

impl Client {
    fn bytes(&self, method: &str, path: &str, content_type: &str, body: &[u8]) -> Vec<u8> {
        let mut head = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {}\r\n\
             MCP-Protocol-Version: 2025-06-18\r\n",
            self.address.port(),
            self.token
        );
        if !body.is_empty() {
            head.push_str(&format!(
                "Content-Type: {content_type}\r\nContent-Length: {}\r\n",
                body.len()
            ));
        }
        head.push_str("\r\n");
        let mut bytes = head.into_bytes();
        bytes.extend_from_slice(body);
        bytes
    }

    fn open(&self, bytes: &[u8]) -> TcpStream {
        let mut conn = TcpStream::connect(self.address).unwrap();
        conn.set_read_timeout(Some(Duration::from_secs(20)))
            .unwrap();
        conn.write_all(bytes).unwrap();
        conn
    }

    fn send(&self, bytes: &[u8]) -> Answer {
        let mut conn = self.open(bytes);
        let mut read = Vec::new();
        let _ = conn.read_to_end(&mut read);
        parse(&read)
    }

    fn api(&self, method: &str, path: &str) -> Answer {
        self.send(&self.bytes(method, path, "", b""))
    }

    fn mcp(&self, message: &Json) -> Answer {
        let body = message.to_string();
        self.send(&self.bytes("POST", "/mcp", "application/json", body.as_bytes()))
    }

    fn call(&self, id: Json, tool: &str, arguments: Json, token: Option<&str>) -> Answer {
        self.mcp(&call_message(id, tool, arguments, token))
    }

    /// A call whose stream is read to its head and then dropped: the run it names.
    fn call_and_drop(&self, tool: &str, arguments: Json) -> String {
        let body = call_message(json!(1), tool, arguments, Some("p")).to_string();
        let mut conn = self.open(&self.bytes("POST", "/mcp", "application/json", body.as_bytes()));
        let mut bytes = Vec::new();
        let mut piece = [0u8; 1024];
        while !bytes.windows(4).any(|w| w == b"\r\n\r\n") {
            let read = conn.read(&mut piece).unwrap();
            assert!(read > 0, "closed before its head");
            bytes.extend_from_slice(&piece[..read]);
        }
        let answer = parse(&bytes);
        assert_eq!(answer.status, 200, "{}", answer.head);
        answer.header("whizbang-run").expect("a run named")
    }

    fn upload(&self, png: &[u8], query: &str) -> Answer {
        self.send(&self.bytes("POST", &format!("/v1/uploads{query}"), "image/png", png))
    }

    /// Polls a run until it says this state, 20 seconds at most.
    fn until_state(&self, run: &str, state: &str) -> Json {
        for _ in 0..4000 {
            let polled = self.api("GET", &format!("/v1/runs/{run}"));
            assert_eq!(polled.status, 200, "{}", polled.text());
            if polled.json()["state"] == state {
                return polled.json();
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("the run never said {state}");
    }
}

fn call_message(id: Json, tool: &str, arguments: Json, token: Option<&str>) -> Json {
    let mut params = json!({"name": tool, "arguments": arguments});
    if let Some(token) = token {
        params["_meta"] = json!({"progressToken": token});
    }
    json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": params})
}

/// A PNG of `width` by `height`, of a few colours.
fn png(width: u32, height: u32) -> Vec<u8> {
    let picture = image::RgbImage::from_fn(width, height, |x, y| {
        image::Rgb([(x * 7) as u8, (y * 5) as u8, ((x + y) * 3) as u8])
    });
    let mut bytes = Vec::new();
    picture
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
    bytes
}

/// The engine fixture, in this process, over a store of its own.
struct Engined {
    scratch: Scratch,
    fixture: Option<Fixture>,
    client: Client,
}

impl Engined {
    fn start(label: &str, tune: impl Fn(&mut Settings)) -> Engined {
        let scratch = Scratch::new(label);
        let root = scratch.path().join("store");
        engine_fixture::write_files(&root, true).unwrap();
        let fixture = Engined::fixture(&root, &tune);
        let client = Client {
            address: fixture.host.address(),
            token: TOKEN.to_owned(),
        };
        Engined {
            scratch,
            fixture: Some(fixture),
            client,
        }
    }

    fn fixture(root: &Path, tune: &dyn Fn(&mut Settings)) -> Fixture {
        let mut settings = Settings::new(SocketAddr::from(([127, 0, 0, 1], 0)));
        settings.grace = Duration::from_millis(500);
        tune(&mut settings);
        engine_fixture::start(root, settings).unwrap()
    }

    fn root(&self) -> PathBuf {
        self.scratch.path().join("store")
    }

    fn kit(&self) -> &whizbang_testkit::playkit::PlayKit {
        &self.fixture.as_ref().unwrap().kit
    }

    fn stop(&mut self) -> Stopped {
        let fixture = self.fixture.take().unwrap();
        fixture.kit.set("release").unwrap();
        fixture.host.stopper().quit();
        fixture.host.wait()
    }

    /// Stops it, and starts it again over the same store and files.
    fn restart(&mut self, tune: impl Fn(&mut Settings)) {
        self.stop();
        let fixture = Engined::fixture(&self.root(), &tune);
        self.client.address = fixture.host.address();
        self.fixture = Some(fixture);
    }

    /// Uploads a PNG of this size: its identity.
    fn picture(&self, width: u32, height: u32) -> String {
        let answer = self.client.upload(&png(width, height), "");
        assert_eq!(answer.status, 201, "{}", answer.text());
        answer.json()["image"].as_str().unwrap().to_owned()
    }

    /// A store's writer once the engine let it go: deletes these runs, and prunes.
    fn delete_and_prune(&self, runs: &[&str]) {
        let opening = wiring::opening(&self.root(), Make::Nothing, wiring::recovery()).unwrap();
        let store = opening.writer().unwrap();
        store.delete(runs).unwrap();
        store.prune().unwrap();
    }
}

impl Drop for Engined {
    fn drop(&mut self) {
        if let Some(fixture) = self.fixture.take() {
            let _ = fixture.kit.set("release");
            let stopper = fixture.host.stopper();
            stopper.quit();
            stopper.quit();
            fixture.host.wait();
        }
    }
}

fn fill_arguments(image: &str, mask: &str, id: Option<&str>) -> Json {
    let mut arguments = json!({"image": image, "mask": mask});
    if let Some(id) = id {
        arguments["request_id"] = json!(id);
    }
    arguments
}

/// Segments a point of a picture: the one mask's identity.
fn mask_of(engined: &Engined, image: &str) -> String {
    let answer = engined.client.call(
        json!(1),
        "segment",
        json!({"image": image, "kind": "point", "query": "8,8", "max_instances": 1}),
        None,
    );
    let result = answer.result();
    assert_eq!(result["isError"], false, "{}", answer.text());
    let text = result["content"][0]["text"].as_str().unwrap();
    text.lines()
        .find_map(|line| line.strip_prefix("mask "))
        .unwrap_or_else(|| panic!("no mask: {text}"))
        .to_owned()
}

// What it does, against the fixture.

#[test]
fn the_offered_revision_initializes_lists_and_calls_to_its_end() {
    let engined = Engined::start("sh-revision", |_| {});
    let client = &engined.client;
    let initialized = client.mcp(&json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                                         "params": {"protocolVersion": "2025-06-18"}}));
    assert_eq!(initialized.status, 200, "{}", initialized.text());
    assert_eq!(
        initialized.json()["result"]["protocolVersion"],
        "2025-06-18"
    );
    let listed = client.mcp(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
    let names: Vec<String> = (listed.json()["result"]["tools"].as_array().unwrap().iter())
        .map(|tool| tool["name"].as_str().unwrap().to_owned())
        .collect();
    for tool in ["segment", "fill", "remove_prompt"] {
        assert!(names.iter().any(|name| name == tool), "{names:?}");
    }
    let image = engined.picture(16, 16);
    let called = client.call(
        json!(3),
        "segment",
        json!({"image": image, "kind": "phrase", "query": "the cat"}),
        None,
    );
    assert_eq!(called.status, 200, "{}", called.text());
    let result = called.result();
    assert_eq!(result["isError"], false, "{}", called.text());
    let run = called.header("whizbang-run").unwrap();
    assert_eq!(result["_meta"]["whizbang/run"], run.as_str());
    assert_eq!(client.until_state(&run, "ended")["outcome"], "completed");
}

#[test]
fn a_call_with_a_progress_token_streams_its_progress_and_its_result_and_names_its_run() {
    let engined = Engined::start("sh-progress", |_| {});
    let image = engined.picture(16, 16);
    let called = engined.client.call(
        json!("a"),
        "segment",
        json!({"image": image, "kind": "phrase", "query": "the cat"}),
        Some("tok"),
    );
    assert_eq!(
        called.header("content-type").as_deref(),
        Some("text/event-stream")
    );
    let run = called.header("whizbang-run").unwrap();
    let events = called.events();
    let (result, progress) = events.split_last().unwrap();
    assert!(!progress.is_empty(), "{}", called.text());
    for (at, note) in progress.iter().enumerate() {
        assert_eq!(note["method"], "notifications/progress");
        assert_eq!(note["params"]["progressToken"], "tok");
        assert_eq!(note["params"]["progress"], at + 1);
    }
    assert!(
        progress[0]["params"]["message"]
            .as_str()
            .unwrap()
            .contains(&run)
    );
    assert_eq!(result["id"], "a");
    assert_eq!(result["result"]["_meta"]["whizbang/run"], run.as_str());
}

#[test]
fn a_call_without_a_progress_token_streams_no_progress_and_still_names_its_run() {
    let engined = Engined::start("sh-no-progress", |_| {});
    let image = engined.picture(16, 16);
    let called = engined.client.call(
        json!(9),
        "segment",
        json!({"image": image, "kind": "phrase", "query": "the cat"}),
        None,
    );
    let run = called.header("whizbang-run").unwrap();
    let events = called.events();
    assert_eq!(events.len(), 1, "{}", called.text());
    assert_eq!(events[0]["id"], 9);
    assert_eq!(events[0]["result"]["_meta"]["whizbang/run"], run.as_str());
}

#[test]
fn an_initialize_asking_another_revision_is_answered_with_2025_06_18() {
    let engined = Engined::start("sh-initialize", |_| {});
    for asked in ["2025-03-26", "2024-11-05", "1999-01-01"] {
        let answer = engined.client.mcp(&json!({"jsonrpc": "2.0", "id": 1,
            "method": "initialize", "params": {"protocolVersion": asked}}));
        assert_eq!(answer.status, 200);
        assert_eq!(answer.json()["result"]["protocolVersion"], "2025-06-18");
        assert!(answer.header("mcp-session-id").is_none());
    }
}

#[test]
fn a_repeat_after_a_restart_after_the_table_let_it_go_and_after_its_run_was_pruned_dispatches_nothing_more()
 {
    let mut engined = Engined::start("sh-repeat-restart", |_| {});
    let image = engined.picture(16, 16);
    let mask = mask_of(&engined, &image);
    let arguments = fill_arguments(&image, &mask, Some("fill-once"));
    let first = engined
        .client
        .call(json!(1), "fill", arguments.clone(), None);
    assert_eq!(first.result()["isError"], false, "{}", first.text());
    let run = first.header("whizbang-run").unwrap();
    assert_eq!(engined.kit().requests(), 2);
    // After a restart: the server's table is new, and holds nothing of the run.
    engined.restart(|_| {});
    let again = engined
        .client
        .call(json!(2), "fill", arguments.clone(), None);
    assert_eq!(again.status, 200, "{}", again.text());
    assert_eq!(again.header("whizbang-run"), Some(run.clone()));
    assert_eq!(again.result()["isError"], false, "{}", again.text());
    assert_eq!(engined.kit().requests(), 0, "a new kit's count");
    // After its run was deleted and the store pruned: still its run, and nothing sent.
    engined.stop();
    engined.delete_and_prune(&[&run]);
    engined.restart_after_stop();
    let pruned = engined.client.call(json!(3), "fill", arguments, None);
    assert_eq!(pruned.header("whizbang-run"), Some(run.clone()));
    assert_eq!(pruned.result()["_meta"]["whizbang/run"], run.as_str());
    assert!(
        pruned.text().contains("no longer in the store"),
        "{}",
        pruned.text()
    );
    assert_eq!(engined.kit().requests(), 0);
}

impl Engined {
    /// Starts it again after [`Engined::stop`].
    fn restart_after_stop(&mut self) {
        let fixture = Engined::fixture(&self.root(), &|_| {});
        self.client.address = fixture.host.address();
        self.fixture = Some(fixture);
    }
}

#[test]
fn the_same_request_id_with_other_arguments_is_a_conflict_after_a_restart_and_after_a_prune() {
    let mut engined = Engined::start("sh-conflict", |_| {});
    let image = engined.picture(16, 16);
    let other = engined.picture(12, 12);
    let first = engined.client.call(
        json!(1),
        "segment",
        json!({"image": image, "kind": "phrase", "query": "a cat", "request_id": "seg-1"}),
        None,
    );
    let run = first.header("whizbang-run").unwrap();
    let changed =
        json!({"image": other, "kind": "phrase", "query": "a cat", "request_id": "seg-1"});
    engined.restart(|_| {});
    let conflict = engined
        .client
        .call(json!(2), "segment", changed.clone(), None);
    assert_eq!(
        conflict.json()["result"]["isError"],
        true,
        "{}",
        conflict.text()
    );
    assert!(conflict.text().contains("conflict"), "{}", conflict.text());
    engined.stop();
    engined.delete_and_prune(&[&run]);
    engined.restart_after_stop();
    let conflict = engined.client.call(json!(3), "segment", changed, None);
    assert!(conflict.text().contains("conflict"), "{}", conflict.text());
    assert_eq!(engined.kit().requests(), 0);
}

#[test]
fn a_repeat_is_answered_at_capacity_and_when_its_backend_is_no_longer_ready() {
    let mut engined = Engined::start("sh-repeat-ready", |settings| settings.calls = 1);
    let image = engined.picture(16, 16);
    let mask = mask_of(&engined, &image);
    let arguments = fill_arguments(&image, &mask, Some("fill-ready"));
    let first = engined
        .client
        .call(json!(1), "fill", arguments.clone(), None);
    let run = first.header("whizbang-run").unwrap();
    // At capacity: the one place is held by a run that waits.
    engined.kit().set("hold").unwrap();
    let held = engined.client.call_and_drop(
        "segment",
        json!({"image": image, "kind": "phrase", "query": "a dog"}),
    );
    let fifth = engined.client.call(
        json!(2),
        "segment",
        json!({"image": image, "kind": "phrase", "query": "a cow"}),
        None,
    );
    assert_eq!(fifth.status, 429, "{}", fifth.text());
    let again = engined
        .client
        .call(json!(3), "fill", arguments.clone(), None);
    assert_eq!(again.header("whizbang-run"), Some(run.clone()));
    assert_eq!(again.result()["isError"], false, "{}", again.text());
    engined.kit().set("release").unwrap();
    engined.client.until_state(&held, "ended");
    // The filler's key taken away: its backend is no longer ready, and the repeat is
    // still its run.
    engined.stop();
    engine_fixture::write_files(&engined.root(), false).unwrap();
    engined.restart_after_stop();
    let unready = engined.client.call(json!(4), "fill", arguments, None);
    assert_eq!(unready.header("whizbang-run"), Some(run));
    assert_eq!(unready.result()["isError"], false, "{}", unready.text());
}

#[test]
fn a_call_repeated_by_request_id_after_its_stream_dropped_follows_the_same_run_and_spends_once() {
    let engined = Engined::start("sh-follow", |_| {});
    let image = engined.picture(16, 16);
    let mask = mask_of(&engined, &image);
    engined.kit().set("hold").unwrap();
    let arguments = fill_arguments(&image, &mask, Some("fill-follow"));
    let run = engined.client.call_and_drop("fill", arguments.clone());
    engined.client.until_state(&run, "running");
    // Followed again while it waits, then let go: the second caller gets its result.
    let mut conn = engined.client.open(
        &engined.client.bytes(
            "POST",
            "/mcp",
            "application/json",
            call_message(json!(2), "fill", arguments, None)
                .to_string()
                .as_bytes(),
        ),
    );
    let mut head = Vec::new();
    let mut piece = [0u8; 512];
    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
        let read = conn.read(&mut piece).unwrap();
        assert!(read > 0);
        head.extend_from_slice(&piece[..read]);
    }
    assert_eq!(parse(&head).header("whizbang-run"), Some(run.clone()));
    engined.kit().set("release").unwrap();
    let mut rest = Vec::new();
    let _ = conn.read_to_end(&mut rest);
    head.extend_from_slice(&rest);
    let answer = parse(&head);
    assert_eq!(answer.result()["isError"], false, "{}", answer.text());
    assert_eq!(answer.result()["_meta"]["whizbang/run"], run.as_str());
    // One segment and one fill were sent: the repeat sent nothing.
    assert_eq!(engined.kit().requests(), 2);
}

#[test]
fn a_run_whose_stream_dropped_is_read_and_cancelled_by_its_id_and_commits_nothing() {
    let engined = Engined::start("sh-cancel", |_| {});
    let image = engined.picture(16, 16);
    let mask = mask_of(&engined, &image);
    engined.kit().set("hold").unwrap();
    let run = engined
        .client
        .call_and_drop("fill", fill_arguments(&image, &mask, None));
    let running = engined.client.until_state(&run, "running");
    assert_eq!(running["run"], run.as_str());
    let cancel = engined
        .client
        .api("POST", &format!("/v1/runs/{run}/cancel"));
    assert_eq!(cancel.status, 202, "{}", cancel.text());
    assert_eq!(cancel.json()["state"], "stopping");
    let ended = engined.client.until_state(&run, "ended");
    assert_eq!(ended["outcome"], "cancelled", "{ended}");
    assert_eq!(ended["outputs"], json!([]));
    // A cancel of it once ended is answered with its end.
    let again = engined
        .client
        .api("POST", &format!("/v1/runs/{run}/cancel"));
    assert_eq!(again.status, 200);
    assert_eq!(again.json()["outcome"], "cancelled");
}

#[test]
fn two_clients_using_the_same_json_rpc_id_each_get_their_own_run() {
    let engined = Engined::start("sh-same-id", |_| {});
    let image = engined.picture(16, 16);
    let arguments = json!({"image": image, "kind": "phrase", "query": "a cat"});
    let one = engined
        .client
        .call(json!(1), "segment", arguments.clone(), None);
    let two = engined.client.call(json!(1), "segment", arguments, None);
    let (one, two) = (
        one.header("whizbang-run").unwrap(),
        two.header("whizbang-run").unwrap(),
    );
    assert_ne!(one, two);
    assert_eq!(engined.kit().requests(), 2);
}

#[test]
fn an_upload_is_named_by_a_call_and_its_result_is_downloaded() {
    let engined = Engined::start("sh-pictures", |_| {});
    let image = engined.picture(16, 16);
    // The same picture uploaded again is the same identity.
    assert_eq!(engined.picture(16, 16), image);
    // Named: the picture and a phrase; clicked: a point's one mask, then a fill.
    let removed = engined.client.call(
        json!(1),
        "remove_prompt",
        json!({"image": image, "prompt": "the cat"}),
        None,
    );
    assert_eq!(removed.result()["isError"], false, "{}", removed.text());
    let mask = mask_of(&engined, &image);
    let filled = engined
        .client
        .call(json!(2), "fill", fill_arguments(&image, &mask, None), None);
    let text = filled.result()["content"][0]["text"]
        .as_str()
        .unwrap()
        .to_owned();
    let result = text
        .lines()
        .find_map(|line| line.strip_prefix("image "))
        .unwrap_or_else(|| panic!("{text}"))
        .to_owned();
    for id in [&image, &mask, &result] {
        let download = engined.client.api("GET", &format!("/v1/artifacts/{id}"));
        assert_eq!(download.status, 200, "{id}: {}", download.text());
        assert_eq!(
            download.header("content-type").as_deref(),
            Some("image/png")
        );
        let decoded = image::load_from_memory(&download.body).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (16, 16));
    }
    // A mask uploaded for its image, and downloaded as it was sent.
    let grey = image::GrayImage::from_fn(16, 16, |x, _| image::Luma([if x < 8 { 255 } else { 0 }]));
    let mut sent = Vec::new();
    grey.write_to(
        &mut std::io::Cursor::new(&mut sent),
        image::ImageFormat::Png,
    )
    .unwrap();
    let uploaded = engined.client.upload(&sent, &format!("?mask_of={image}"));
    assert_eq!(uploaded.status, 201, "{}", uploaded.text());
    let id = uploaded.json()["mask"].as_str().unwrap().to_owned();
    let download = engined.client.api("GET", &format!("/v1/artifacts/{id}"));
    let back = image::load_from_memory(&download.body).unwrap().to_luma8();
    assert_eq!(back, grey);
    // An identity the store does not hold.
    let none = format!("/v1/artifacts/sha256:{}", "0".repeat(64));
    assert_eq!(engined.client.api("GET", &none).status, 404);
}

#[test]
fn a_signal_cancels_every_run_accounts_the_ledger_and_releases_the_store() {
    // In process, through the same stop a signal asks for: a paid run that waits is
    // cancelled and waited for, and the store is let go.
    let mut engined = Engined::start("sh-stop", |settings| {
        settings.grace = Duration::from_secs(10);
    });
    let image = engined.picture(16, 16);
    let mask = mask_of(&engined, &image);
    engined.kit().set("hold").unwrap();
    let run = engined
        .client
        .call_and_drop("fill", fill_arguments(&image, &mask, None));
    engined.client.until_state(&run, "running");
    let fixture = engined.fixture.take().unwrap();
    fixture.host.stopper().quit();
    let started = Instant::now();
    let stopped = fixture.host.wait();
    assert_eq!(stopped.left, 0, "{:?}", started.elapsed());
    let opening = wiring::opening(&engined.root(), Make::Nothing, wiring::recovery());
    assert!(opening.is_ok(), "{:?}", opening.err());
    drop(opening);
    // Its record says cancelled, and the ledger holds no reservation left open.
    let shown = run_binary(&[
        "runs",
        "show",
        &run,
        "--store",
        engined.root().to_str().unwrap(),
    ]);
    assert!(shown.contains("status: cancelled"), "{shown}");
    // Every reservation was released.
    let spend = run_binary(&[
        "spend",
        "status",
        "--store",
        engined.root().to_str().unwrap(),
    ]);
    assert!(spend.contains("were not released: 0"), "{spend}");
    // The shipped binary: a hang-up stops it, and the store is free.
    let scratch = Scratch::new("sh-stop-binary");
    let root = scratch.path().join("store");
    let served = Served::start(
        &root,
        &[],
        &[("ENGINE_TOKEN", TOKEN)],
        &["--token-env", "ENGINE_TOKEN"],
    );
    let client = served.client();
    assert_eq!(
        client
            .mcp(&json!({"jsonrpc": "2.0", "id": 1, "method": "ping"}))
            .status,
        200
    );
    served.signal("HUP");
    let (code, out, err) = served.ended();
    assert_eq!(code, Some(0), "{err}");
    assert!(out.contains("every run it started has ended"), "{out}");
    assert!(!err.contains(TOKEN) && !out.contains(TOKEN));
    let opening = wiring::opening(&root, Make::Nothing, wiring::recovery());
    assert!(opening.is_ok(), "{:?}", opening.err());
}

/// The binary run to its end: its standard output, and its error output after it.
fn run_binary(args: &[&str]) -> String {
    let mut binary = binary();
    let output = binary.args(args).output().unwrap();
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

// The shipped binary.

/// The shipped binary, as every test starts it: through the test kit's one function.
fn binary() -> whizbang_testkit::binary::Binary {
    whizbang_testkit::binary::command(env!("CARGO_BIN_EXE_whizbang"))
}

/// `whizbang serve --http 127.0.0.1:0`, started, its first line of error output read.
struct Served {
    child: std::process::Child,
    _binary: whizbang_testkit::binary::Binary,
    address: SocketAddr,
    first: String,
}

impl Served {
    fn start(root: &Path, more: &[&str], variables: &[(&str, &str)], token: &[&str]) -> Served {
        let mut binary = binary();
        binary
            .args([
                "serve",
                "--store",
                root.to_str().unwrap(),
                "--http",
                "127.0.0.1:0",
            ])
            .args(token)
            .args(more)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (name, value) in variables {
            binary.env(name, value);
        }
        let mut child = binary.spawn().unwrap();
        let mut first = String::new();
        BufReader::new(child.stderr.as_mut().unwrap())
            .read_line(&mut first)
            .unwrap();
        let address = first
            .split("listening on ")
            .nth(1)
            .and_then(|rest| rest.split(';').next())
            .and_then(|address| address.parse().ok())
            .unwrap_or_else(|| panic!("no address: {first}"));
        Served {
            child,
            _binary: binary,
            address,
            first,
        }
    }

    fn client(&self) -> Client {
        Client {
            address: self.address,
            token: TOKEN.to_owned(),
        }
    }

    fn signal(&self, name: &str) {
        let sent = std::process::Command::new("kill")
            .args([format!("-{name}"), self.child.id().to_string()])
            .status()
            .unwrap();
        assert!(sent.success());
    }

    /// Waits for it, 20 seconds at most: its exit code and what it printed.
    fn ended(mut self) -> (Option<i32>, String, String) {
        let limit = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                let mut out = String::new();
                let mut err = self.first.clone();
                let _ = self.child.stdout.take().unwrap().read_to_string(&mut out);
                let _ = self.child.stderr.take().unwrap().read_to_string(&mut err);
                return (status.code(), out, err);
            }
            if Instant::now() > limit {
                let _ = self.child.kill();
                panic!("whizbang serve --http did not end");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

/// The binary started with these words, which it refuses: its exit code and its error
/// output.
fn refused(args: &[&str], variables: &[(&str, &str)]) -> (Option<i32>, String) {
    let mut binary = binary();
    binary.args(args).stdin(Stdio::null());
    for (name, value) in variables {
        binary.env(name, value);
    }
    let output = binary.output().unwrap();
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn an_address_off_loopback_is_refused_without_off_loopback_and_a_host_name() {
    let scratch = Scratch::new("sh-off-loopback");
    let store = scratch.path().join("store");
    let store = store.to_str().unwrap();
    let base = ["serve", "--store", store, "--token-env", "ENGINE_TOKEN"];
    let token = [("ENGINE_TOKEN", TOKEN)];
    for more in [
        vec!["--http", "0.0.0.0:0"],
        vec!["--http", "0.0.0.0:0", "--off-loopback"],
        vec!["--http", "0.0.0.0:0", "--host-name", "engine.example"],
    ] {
        let args: Vec<&str> = base.iter().copied().chain(more.iter().copied()).collect();
        let (code, err) = refused(&args, &token);
        assert_eq!(code, Some(1), "{more:?}: {err}");
        assert!(err.contains("not a loopback address"), "{err}");
        assert!(err.contains("--off-loopback"), "{err}");
    }
    // Nothing was made: the refusal comes before the store is opened.
    assert!(!scratch.path().join("store").exists());
    // A flag of the engine without --http is refused by the parser.
    let (code, err) = refused(&["serve", "--store", store, "--off-loopback"], &[]);
    assert_eq!(code, Some(2), "{err}");
}

#[test]
fn a_token_file_others_can_read_or_a_short_token_is_refused_at_start() {
    let scratch = Scratch::new("sh-token-file");
    let store = scratch.path().join("store");
    let store = store.to_str().unwrap();
    let file = scratch.path().join("token");
    std::fs::write(&file, format!("{TOKEN}\n")).unwrap();
    let file = file.to_str().unwrap().to_owned();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for mode in [0o640, 0o604, 0o620, 0o644] {
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode)).unwrap();
            let (code, err) = refused(
                &[
                    "serve",
                    "--store",
                    store,
                    "--http",
                    "127.0.0.1:0",
                    "--token-file",
                    &file,
                ],
                &[],
            );
            assert_eq!(code, Some(1), "{mode:o}: {err}");
            assert!(err.contains("group or anyone"), "{err}");
            assert!(!err.contains(TOKEN), "{err}");
        }
    }
    for short in [
        "",
        "short",
        &"a".repeat(31),
        "has a space in it and is long enough!!",
    ] {
        let (code, err) = refused(
            &[
                "serve",
                "--store",
                store,
                "--http",
                "127.0.0.1:0",
                "--token-env",
                "ENGINE_TOKEN",
            ],
            &[("ENGINE_TOKEN", short)],
        );
        assert_eq!(code, Some(1), "{short:?}: {err}");
        assert!(err.contains("token"), "{err}");
        assert!(short.is_empty() || !err.contains(short), "{err}");
    }
    // Its owner's alone: taken, and the start line never names it.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let root = scratch.path().join("store");
    let served = Served::start(&root, &[], &[], &["--token-file", &file]);
    assert!(served.first.contains("on loopback"), "{}", served.first);
    assert!(!served.first.contains(TOKEN));
    served.signal("TERM");
    let (code, _, err) = served.ended();
    assert_eq!(code, Some(0), "{err}");
}

#[test]
fn with_neither_token_flag_or_with_both_the_start_is_refused() {
    let scratch = Scratch::new("sh-token-flags");
    let store = scratch.path().join("store");
    let store = store.to_str().unwrap();
    let file = scratch.path().join("token");
    std::fs::write(&file, TOKEN).unwrap();
    let file = file.to_str().unwrap();
    let token = [("ENGINE_TOKEN", TOKEN)];
    for more in [
        vec![],
        vec!["--token-env", "ENGINE_TOKEN", "--token-file", file],
    ] {
        let args: Vec<&str> = ["serve", "--store", store, "--http", "127.0.0.1:0"]
            .into_iter()
            .chain(more.iter().copied())
            .collect();
        let (code, err) = refused(&args, &token);
        assert_eq!(code, Some(1), "{more:?}: {err}");
        assert!(
            err.contains("exactly one of --token-env and --token-file"),
            "{err}"
        );
    }
    // A variable that is not set.
    let (code, err) = refused(
        &[
            "serve",
            "--store",
            store,
            "--http",
            "127.0.0.1:0",
            "--token-env",
            "ENGINE_TOKEN",
        ],
        &[],
    );
    assert_eq!(code, Some(1), "{err}");
    assert!(err.contains("is not set"), "{err}");
}
