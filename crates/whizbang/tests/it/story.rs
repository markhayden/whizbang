//! `story::…`: user story 1 on fakes (`SPEC.md` section 1; plan m6, task D1, item 4). A
//! hosted editor's server runs the engine API with no GPU and no Python; segmentation runs
//! on SAM 3 on a GPU host, the fill on Bria through fal; the app calls two or three actions
//! and writes no model code. Here the engine is the engine fixture (`examples/
//! engine_fixture.rs`: the engine API as it ships, with the binary's wiring and door), its
//! segmenter of the kind `remote` at the fake host (the pack's own front on loopback over
//! HTTPS, with the pack's fake models behind it), its filler fal's at a server on loopback
//! that answers as fal does, both under the test's root. The app is this test's client,
//! which speaks only HTTP to the engine. Every token and key is made up; nothing leaves the
//! machine.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value as Json, json};
use whizbang_backends::service::ServiceKind;
use whizbang_core::config::Kinds;
use whizbang_core::store::Reader;
use whizbang_core::store::fs::RealFs;
use whizbang_serve::engine::Settings;
use whizbang_testkit::scratch::Scratch;

use crate::anthropic::{KEY_VARIABLE, made_up_key, record_of, serving};
use crate::fal::{FILL_CLASSES, answered};
use crate::remote::host::{self, Host, named};
use crate::serve_http::engine_fixture::{self, Backends, TOKEN};

/// The variable the env file sets the front's token in, as the guide's config names it.
const SEGMENT_TOKEN: &str = "SEGMENT_TOKEN";
/// The picture's size: that of the picture fal's fake answers with.
const WIDTH: u32 = 64;
const HEIGHT: u32 = 48;

/// What the app reads of an answer: its status, a header, its body.
struct Answer {
    status: u16,
    head: String,
    body: Vec<u8>,
}

impl Answer {
    fn header(&self, name: &str) -> Option<String> {
        self.head.lines().skip(1).find_map(|line| {
            let (named, value) = line.split_once(':')?;
            named
                .eq_ignore_ascii_case(name)
                .then(|| value.trim().to_owned())
        })
    }

    fn json(&self) -> Json {
        serde_json::from_slice(&self.body).unwrap_or(Json::Null)
    }

    /// A tool's result, the last message of its event stream: its text.
    fn said(&self) -> String {
        let text = String::from_utf8_lossy(&self.body).into_owned();
        let last = text
            .split("\n\n")
            .filter_map(|event| {
                let data = event.lines().find_map(|line| line.strip_prefix("data: "))?;
                serde_json::from_str::<Json>(data).ok()
            })
            .last()
            .unwrap_or(Json::Null);
        assert_eq!(last["result"]["isError"], false, "{text}");
        last["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("{text}"))
            .to_owned()
    }
}

/// The app: an HTTP client of the engine and its token, and nothing else.
struct App {
    address: SocketAddr,
}

impl App {
    fn send(&self, method: &str, path: &str, content_type: &str, body: &[u8]) -> Answer {
        let mut head = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {TOKEN}\r\n\
             MCP-Protocol-Version: 2025-06-18\r\n",
            self.address.port()
        );
        if !body.is_empty() {
            head.push_str(&format!(
                "Content-Type: {content_type}\r\nContent-Length: {}\r\n",
                body.len()
            ));
        }
        head.push_str("\r\n");
        let mut conn = TcpStream::connect(self.address).unwrap();
        conn.set_read_timeout(Some(Duration::from_secs(20)))
            .unwrap();
        conn.write_all(head.as_bytes()).unwrap();
        conn.write_all(body).unwrap();
        let mut read = Vec::new();
        let _ = conn.read_to_end(&mut read);
        let end = read
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .unwrap_or_else(|| panic!("no answer: {:?}", String::from_utf8_lossy(&read)));
        let head = String::from_utf8_lossy(&read[..end]).into_owned();
        Answer {
            status: head.split(' ').nth(1).unwrap().parse().unwrap(),
            head,
            body: read[end + 4..].to_vec(),
        }
    }

    /// `POST /v1/uploads`: the picture's identity.
    fn upload(&self, png: &[u8]) -> String {
        let answer = self.send("POST", "/v1/uploads", "image/png", png);
        assert_eq!(
            answer.status,
            201,
            "{}",
            String::from_utf8_lossy(&answer.body)
        );
        answer.json()["image"].as_str().unwrap().to_owned()
    }

    /// A `tools/call`: the run it names, and what its result says.
    fn call(&self, tool: &str, arguments: Json) -> (String, String) {
        let message = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {"name": tool, "arguments": arguments}});
        let answer = self.send(
            "POST",
            "/mcp",
            "application/json",
            message.to_string().as_bytes(),
        );
        assert_eq!(answer.status, 200, "{}", answer.head);
        let run = answer.header("whizbang-run").expect("the run is named");
        (run, answer.said())
    }

    /// `GET /v1/artifacts/<id>`: the PNG's pixels' size.
    fn download(&self, id: &str) -> (u32, u32) {
        let answer = self.send("GET", &format!("/v1/artifacts/{id}"), "", b"");
        assert_eq!(answer.status, 200, "{id}");
        assert_eq!(answer.header("content-type").as_deref(), Some("image/png"));
        let decoded = image::load_from_memory(&answer.body).unwrap();
        (decoded.width(), decoded.height())
    }
}

/// The line of a tool's text that starts with `label `: what follows it.
fn line(said: &str, label: &str) -> String {
    said.lines()
        .find_map(|line| line.strip_prefix(&format!("{label} ")))
        .unwrap_or_else(|| panic!("no {label}: {said}"))
        .to_owned()
}

/// A picture of the fake answer's size: red in its left half, where `fake-ok`'s phrase
/// finds its thing.
fn picture() -> Vec<u8> {
    let made = image::RgbImage::from_fn(WIDTH, HEIGHT, |x, y| {
        image::Rgb([if x < WIDTH / 2 { 220 } else { 10 }, (y * 3) as u8, 40])
    });
    let mut bytes = Vec::new();
    made.write_to(
        &mut std::io::Cursor::new(&mut bytes),
        image::ImageFormat::Png,
    )
    .unwrap();
    bytes
}

/// Every record the store holds, by its run's id.
fn records(root: &std::path::Path) -> Vec<(String, Json)> {
    let reader = Reader::open(root, Arc::new(RealFs)).unwrap();
    let runs: Vec<String> = reader.snapshot().unwrap().runs().keys().cloned().collect();
    drop(reader);
    runs.into_iter()
        .map(|run| {
            let record = record_of(root, &run);
            (run, record)
        })
        .collect()
}

#[test]
fn a_hosted_editor_uploads_names_and_clicks_and_downloads_with_no_local_worker() {
    let model = named(None, "fake-ok");
    let front = Host::serving(std::slice::from_ref(&model));
    let (fal, _log) = serving(&front.pki, vec![answered(), answered()]);
    let fal_origin = format!("https://127.0.0.1:{}", fal.port());
    let fill_classes: Vec<String> = FILL_CLASSES.iter().map(|c| format!("\"{c}\"")).collect();
    // The config a guide's reader writes: the remote segmenter at the front, priced as the
    // plan's example; fal's filler; each origin approved for what it is sent.
    let config = format!(
        "[backends.segment]\nkind = \"remote\"\nurl = \"{base}\"\ntoken_env = \"{SEGMENT_TOKEN}\"\n\
         model = \"{model}\"\n\n[backends.segment.settings]\nprice_microusd_per_second = 300\n\
         host_limit_s = 600\n\n\
         [backends.fill]\nkind = \"service\"\nprovider = \"fal\"\nmodel = \"bria-eraser\"\n\
         key_env = \"{KEY_VARIABLE}\"\n\n[spend]\ncall_cap_usd = 0.50\n\n\
         [egress.recipients.gpu]\norigin = \"{base}\"\nclasses = [\"source_images\", \"prompts\"]\n\
         probe = true\n\n[egress.recipients.fal]\norigin = \"{fal_origin}\"\nclasses = [{}]\n",
        fill_classes.join(", "),
        base = front.base(),
    );
    let env = format!(
        "{SEGMENT_TOKEN}={}\n{KEY_VARIABLE}={}\n",
        host::TOKEN,
        made_up_key()
    );
    let mut kinds = Kinds::new();
    kinds.register(Arc::new(front.kind())).unwrap();
    kinds
        .register(Arc::new(ServiceKind::at(&format!(
            "{fal_origin}/fal-ai/bria/eraser"
        ))))
        .unwrap();
    let backends = Backends {
        config,
        env,
        kinds,
        trusting: front.pki.clone(),
    };
    let scratch = Scratch::new("story-one");
    let root = scratch.path().join("store");
    let fixture = engine_fixture::start_with(
        &root,
        Settings::new(SocketAddr::from(([127, 0, 0, 1], 0))),
        backends,
    )
    .unwrap();
    let app = App {
        address: fixture.host.address(),
    };

    // Named: an upload, `remove_prompt` with the words, a download of its result.
    let image = app.upload(&picture());
    let (named_run, said) = app.call(
        "remove_prompt",
        json!({"image": image, "prompt": "the red thing"}),
    );
    assert_eq!(app.download(&line(&said, "image")), (WIDTH, HEIGHT));
    // Clicked: an upload again (the same picture is the same identity), `segment` with a
    // point and one instance, `fill` with its mask, a download of that.
    assert_eq!(app.upload(&picture()), image);
    let (segment_run, said) = app.call(
        "segment",
        json!({"image": image, "kind": "point", "query": "8,8", "max_instances": 1}),
    );
    let mask = line(&said, "mask");
    let (fill_run, said) = app.call("fill", json!({"image": image, "mask": mask}));
    assert_eq!(app.download(&line(&said, "image")), (WIDTH, HEIGHT));

    // Whizbang started no pack worker: the binary's one worker was never asked.
    assert!(fixture.worker.status().is_none(), "{:?}", fixture.worker);
    assert!(fixture.worker.journal().is_empty());
    fixture.host.stopper().quit();
    fixture.host.wait();
    assert_eq!(fal.finish(), 2, "fal was asked once for each fill");
    assert_eq!(
        front.seen().len(),
        2,
        "the front was asked once for each segment"
    );

    // Each run's record names the front for segment and fal for fill, with their classes;
    // the ledger booked the segment's bound and settled it, and the fill's price.
    let all = records(&root);
    let mut segments = 0;
    let mut fills = 0;
    for (run, record) in &all {
        let action = record["asked"]["action"].as_str().unwrap_or_default();
        let egress = record["egress"].as_array().cloned().unwrap_or_default();
        let requests = record["spend"]["requests"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        match action {
            "segment" => {
                segments += 1;
                assert_eq!(egress.len(), 1, "{run}: {record}");
                assert_eq!(egress[0]["origin"], front.base(), "{run}: {record}");
                assert_eq!(egress[0]["classes"], json!(["source_images", "prompts"]));
                assert_eq!(requests.len(), 1, "{run}: {record}");
                assert_eq!(requests[0]["reserved_micros"], 360_000, "{run}: {record}");
                assert_eq!(requests[0]["settled_micros"], 360_000, "{run}: {record}");
                assert_eq!(record["models"][0]["backend"], "remote", "{run}: {record}");
            }
            "fill" => {
                fills += 1;
                assert_eq!(egress.len(), 1, "{run}: {record}");
                assert_eq!(egress[0]["origin"], fal_origin.as_str(), "{run}: {record}");
                assert_eq!(egress[0]["classes"], json!(FILL_CLASSES));
                assert_eq!(requests.len(), 1, "{run}: {record}");
                let settled = requests[0]["settled_micros"].as_u64().unwrap_or(0);
                assert!(settled > 0, "{run}: the fill's price is booked: {record}");
            }
            _ => assert!(egress.is_empty(), "{run} ({action}) sent nothing: {record}"),
        }
    }
    assert_eq!((segments, fills), (2, 2), "{all:?}");
    for run in [&named_run, &segment_run, &fill_run] {
        assert!(all.iter().any(|(listed, _)| listed == run), "{run}");
    }
}
