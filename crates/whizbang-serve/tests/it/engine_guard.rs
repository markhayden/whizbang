//! `engine_guard::…`: the guard of the engine API (`SPEC.md` section 10, the negative tests;
//! plan m6, task H1, item 7; ADR 0045, "Who may call"), each check shown refusing on a
//! real loopback socket, with no client library and no sleep standing for an event.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use serde_json::{Value as Json, json};
use whizbang_core::contract::{Call, Executor, Request, RunId};
use whizbang_serve::engine::{Secret, threads};

use crate::engine_kit::{Engined, Opened, TOKEN, events, fresh_id, png_saying, upload_bytes};
use crate::{Answer, read_answer, request, send};

/// The guard's headers, which every answer carries, and the one it never carries.
fn guarded_headers(answer: &Answer) {
    assert_eq!(answer.header("cache-control").as_deref(), Some("no-store"));
    assert_eq!(
        answer.header("x-content-type-options").as_deref(),
        Some("nosniff")
    );
    assert!(
        !answer.head.to_ascii_lowercase().contains("access-control-"),
        "{}",
        answer.head
    );
}

fn ping() -> Json {
    json!({"jsonrpc": "2.0", "id": 1, "method": "ping"})
}

fn with_headers(engined: &Engined, headers: &[(&str, &str)], body: &Json) -> Answer {
    let body = body.to_string();
    let mut all = headers.to_vec();
    all.push(("Content-Type", "application/json"));
    all.push(("MCP-Protocol-Version", "2025-06-18"));
    send(
        engined.address,
        &request("POST", "/mcp", &all, Some(body.as_bytes())),
    )
}

fn call(tool: &str, arguments: Json) -> Json {
    json!({"jsonrpc": "2.0", "id": 7, "method": "tools/call",
           "params": {"name": tool, "arguments": arguments}})
}

/// Starts four runs of `waits`, each under a request id of its own, their streams held
/// open: the streams, and the ids.
fn four_streams(engined: &Engined) -> Vec<(Opened, String)> {
    (0..4)
        .map(|_| {
            let id = fresh_id();
            let opened = engined.open_call("waits", json!({"request_id": id}));
            assert_eq!(opened.status, 200, "{}", opened.head);
            (opened, id)
        })
        .collect()
}

/// A picture to upload: a real PNG, 2 by 2.
fn small_png() -> Vec<u8> {
    let mut png = png_saying(2, 2);
    png.extend_from_slice(b"rest");
    png
}

#[test]
fn a_call_without_the_token_is_refused_401_and_names_nothing() {
    let engined = Engined::start("eg-no-token", |_| {});
    let host = engined.host_header();
    let store = engined.scratch.path().to_string_lossy().into_owned();
    for headers in [
        vec![("Host", host.as_str())],
        vec![("Host", host.as_str()), ("Authorization", "Basic abc")],
        vec![("Host", host.as_str()), ("Authorization", "Bearer ")],
        vec![("Host", host.as_str()), ("Authorization", "bearer x")],
    ] {
        let answer = with_headers(&engined, &headers, &ping());
        assert_eq!(answer.status, 401, "{}", answer.whole());
        let said = answer.whole();
        assert!(!said.contains(&store), "{said}");
        assert!(!said.contains(env!("CARGO_PKG_VERSION")), "{said}");
        assert!(!said.contains(TOKEN), "{said}");
        guarded_headers(&answer);
    }
    // Every route is behind it, whatever the method.
    for (method, path) in [
        ("GET", "/v1/runs/x"),
        ("POST", "/v1/runs/x/cancel"),
        ("GET", "/v1/artifacts/x"),
        ("POST", "/v1/uploads"),
        ("GET", "/nothing-here"),
    ] {
        let answer = send(
            engined.address,
            &request(method, path, &[("Host", &host)], None),
        );
        assert_eq!(answer.status, 401, "{method} {path}");
    }
    assert_eq!(engined.mcp(&ping()).status, 200);
}

#[test]
fn a_call_with_a_wrong_token_is_refused_401() {
    let engined = Engined::start("eg-wrong-token", |_| {});
    let host = engined.host_header();
    let mut close = TOKEN.to_owned();
    let last = close.pop().unwrap();
    close.push(if last == '0' { '1' } else { '0' });
    let longer = format!("{TOKEN}0");
    for presented in [
        "another-made-up-token-of-forty-chars-ok".to_owned(),
        close,
        TOKEN.to_uppercase(),
        TOKEN[1..].to_owned(),
        longer,
    ] {
        let bearer = format!("Bearer {presented}");
        let headers = [("Host", host.as_str()), ("Authorization", bearer.as_str())];
        let answer = with_headers(&engined, &headers, &ping());
        assert_eq!(answer.status, 401, "{}", answer.whole());
    }
}

#[test]
fn a_request_whose_host_is_not_one_of_the_servers_names_is_refused_421() {
    let engined = Engined::start("eg-host", |settings| {
        settings.host_names = vec!["engine.example".to_owned()];
    });
    let port = engined.address.port();
    let bearer = format!("Bearer {TOKEN}");
    let other_port = format!("127.0.0.1:{}", port.wrapping_add(1));
    let rebound = format!("attacker.example:{port}");
    let near = format!("engine.example.attacker:{port}");
    for host in [
        rebound.as_str(),
        other_port.as_str(),
        "127.0.0.1",
        near.as_str(),
        "",
    ] {
        let headers = [("Host", host), ("Authorization", bearer.as_str())];
        let answer = with_headers(&engined, &headers, &ping());
        assert_eq!(answer.status, 421, "{host}: {}", answer.whole());
        guarded_headers(&answer);
    }
    // No Host at all.
    let answer = with_headers(&engined, &[("Authorization", bearer.as_str())], &ping());
    assert_eq!(answer.status, 421);
    // Each of its names: loopback's three at its port, and the name it was given, with
    // its port or without.
    let named = [
        format!("127.0.0.1:{port}"),
        format!("localhost:{port}"),
        format!("[::1]:{port}"),
        "engine.example".to_owned(),
        format!("engine.example:{port}"),
    ];
    for host in &named {
        let headers = [("Host", host.as_str()), ("Authorization", bearer.as_str())];
        let answer = with_headers(&engined, &headers, &ping());
        assert_eq!(answer.status, 200, "{host}: {}", answer.whole());
    }
}

#[test]
fn a_request_from_an_origin_not_listed_is_refused_403_with_no_allow_origin_header() {
    let engined = Engined::start("eg-origin", |settings| {
        settings.allow_origins = vec!["https://app.example".to_owned()];
    });
    let host = engined.host_header();
    let bearer = format!("Bearer {TOKEN}");
    let own = format!("http://{host}");
    for origin in [
        "https://attacker.example",
        "https://app.example.attacker",
        "http://app.example",
        own.as_str(),
        "null",
    ] {
        let headers = [
            ("Host", host.as_str()),
            ("Authorization", bearer.as_str()),
            ("Origin", origin),
        ];
        let answer = with_headers(&engined, &headers, &ping());
        assert_eq!(answer.status, 403, "{origin}: {}", answer.whole());
        guarded_headers(&answer);
    }
    // With no origin listed, any origin is refused. One engine at a time in a process.
    drop(engined);
    let unlisted = Engined::start("eg-origin-none", |_| {});
    let host = unlisted.host_header();
    let headers = [
        ("Host", host.as_str()),
        ("Authorization", bearer.as_str()),
        ("Origin", "https://app.example"),
    ];
    assert_eq!(with_headers(&unlisted, &headers, &ping()).status, 403);
}

#[test]
fn a_listed_origin_is_answered_and_a_request_with_no_origin_is_taken() {
    let engined = Engined::start("eg-origin-listed", |settings| {
        settings.allow_origins = vec!["https://app.example".to_owned()];
    });
    let host = engined.host_header();
    let bearer = format!("Bearer {TOKEN}");
    let headers = [
        ("Host", host.as_str()),
        ("Authorization", bearer.as_str()),
        ("Origin", "https://app.example"),
    ];
    let answer = with_headers(&engined, &headers, &ping());
    assert_eq!(answer.status, 200, "{}", answer.whole());
    guarded_headers(&answer);
    // A server's own client sends no Origin.
    assert_eq!(engined.mcp(&ping()).status, 200);
}

#[test]
fn an_upload_over_64_mib_is_refused_413_before_it_is_read_whole() {
    let engined = Engined::start("eg-upload-big", |_| {});
    let host = engined.host_header();
    let bearer = format!("Bearer {TOKEN}");
    let length = (64u64 << 20) + 1;
    let head = format!(
        "POST /v1/uploads HTTP/1.1\r\nHost: {host}\r\nAuthorization: {bearer}\r\n\
         Content-Type: image/png\r\nContent-Length: {length}\r\n\r\n"
    );
    let started = Instant::now();
    let mut conn = TcpStream::connect(engined.address).unwrap();
    // The head alone: nothing of the body is sent, and it is answered.
    conn.write_all(head.as_bytes()).unwrap();
    let answer = read_answer(&mut conn);
    assert_eq!(answer.status, 413, "{}", answer.whole());
    assert!(answer.text().contains("64 MiB"));
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn an_upload_over_the_pixel_limit_is_refused_413_naming_the_limit() {
    let engined = Engined::start("eg-pixels", |_| {});
    // 5,001 by 5,000 is over 25,000,000 by its header alone; nothing more is sent.
    let answer = send(
        engined.address,
        &upload_bytes(&engined, "image/png", &png_saying(5001, 5000), ""),
    );
    assert_eq!(answer.status, 413, "{}", answer.whole());
    assert!(answer.text().contains("25000000"), "{}", answer.text());
    // Inside it, taken.
    let answer = send(
        engined.address,
        &upload_bytes(&engined, "image/png", &small_png(), ""),
    );
    assert_eq!(answer.status, 201, "{}", answer.whole());
    // A lower limit is the one named. One engine at a time in a process.
    drop(engined);
    let low = Engined::start("eg-pixels-low", |settings| settings.pixel_limit = 3);
    let answer = send(
        low.address,
        &upload_bytes(&low, "image/png", &small_png(), ""),
    );
    assert_eq!(answer.status, 413, "{}", answer.whole());
    assert!(answer.text().contains("limit of 3"), "{}", answer.text());
}

#[test]
fn an_upload_of_webp_or_another_type_is_refused_415() {
    let engined = Engined::start("eg-type", |_| {});
    let webp = b"RIFF\0\0\0\0WEBPVP8 ".to_vec();
    for (content_type, body) in [
        ("image/webp", webp.clone()),
        ("image/gif", b"GIF89a".to_vec()),
        ("text/plain", b"hello".to_vec()),
        ("application/json", b"{}".to_vec()),
        // Named a PNG, and none; named a JPEG, and a PNG; named a PNG, and a JPEG.
        ("image/png", webp),
        ("image/jpeg", small_png()),
        (
            "image/png",
            vec![0xff, 0xd8, 0xff, 0xc0, 0, 17, 8, 0, 2, 0, 2],
        ),
    ] {
        let answer = send(
            engined.address,
            &upload_bytes(&engined, content_type, &body, ""),
        );
        assert_eq!(answer.status, 415, "{content_type}: {}", answer.whole());
    }
    // A mask is a grey PNG and nothing else.
    let image = send(
        engined.address,
        &upload_bytes(&engined, "image/png", &small_png(), ""),
    );
    let id = image.json()["image"].as_str().unwrap().to_owned();
    let answer = send(
        engined.address,
        &upload_bytes(
            &engined,
            "image/jpeg",
            &[0xff, 0xd8],
            &format!("?mask_of={id}"),
        ),
    );
    assert_eq!(answer.status, 415, "{}", answer.whole());
}

#[test]
fn a_mask_not_of_its_images_size_is_refused() {
    let engined = Engined::start("eg-mask", |_| {});
    let image = send(
        engined.address,
        &upload_bytes(&engined, "image/png", &small_png(), ""),
    );
    assert_eq!(image.status, 201, "{}", image.whole());
    let id = image.json()["image"].as_str().unwrap().to_owned();
    let query = format!("?mask_of={id}");
    for (width, height) in [(3, 2), (2, 3), (1, 1)] {
        let mut mask = png_saying(width, height);
        mask.extend_from_slice(b"mask");
        let answer = send(
            engined.address,
            &upload_bytes(&engined, "image/png", &mask, &query),
        );
        assert_eq!(answer.status, 400, "{}", answer.whole());
        assert!(answer.text().contains("its image's size exactly"));
    }
    // Of no image the store holds; of no identity at all.
    let none = format!("?mask_of=sha256:{}", "0".repeat(64));
    let answer = send(
        engined.address,
        &upload_bytes(&engined, "image/png", &small_png(), &none),
    );
    assert_eq!(answer.status, 404, "{}", answer.whole());
    let answer = send(
        engined.address,
        &upload_bytes(&engined, "image/png", &small_png(), "?mask_of=../x"),
    );
    assert_eq!(answer.status, 400, "{}", answer.whole());
    // Of its size, taken.
    let mut mask = png_saying(2, 2);
    mask.extend_from_slice(b"mask");
    let answer = send(
        engined.address,
        &upload_bytes(&engined, "image/png", &mask, &query),
    );
    assert_eq!(answer.status, 201, "{}", answer.whole());
    assert!(answer.json()["mask"].is_string());
}

#[test]
fn a_path_in_any_argument_of_any_tool_is_refused_400() {
    let engined = Engined::start("eg-path", |_| {});
    let paths = [
        "/etc/passwd",
        "~/pictures/x.png",
        "C:\\pictures\\x.png",
        "file:///x.png",
        "pictures/../../x.png",
    ];
    for path in paths {
        for arguments in [
            json!({"words": path}),
            json!({"image": path}),
            json!({"request_id": path}),
            json!({"words": "fine", "nested": {"deeper": [path]}}),
        ] {
            for tool in ["described", "quick", "waits"] {
                let answer = engined.mcp(&call(tool, arguments.clone()));
                assert_eq!(answer.status, 400, "{tool} {arguments}: {}", answer.whole());
                assert!(
                    answer.text().contains("reads as a path"),
                    "{}",
                    answer.text()
                );
                assert!(!answer.text().contains(path), "{}", answer.text());
            }
        }
    }
    assert_eq!(engined.ran("quick") + engined.ran("waits"), 0);
    // Words that are no path are taken; an image that is no identity is refused by the
    // tool, and nothing runs.
    let answer = engined.mcp(&call("described", json!({"image": "a cat"})));
    assert_eq!(answer.status, 200, "{}", answer.whole());
    assert_eq!(
        answer.json()["result"]["isError"],
        true,
        "{}",
        answer.text()
    );
}

#[test]
fn a_json_body_over_1_mib_or_deeper_than_the_limit_is_refused() {
    let engined = Engined::start("eg-json", |_| {});
    let host = engined.host_header();
    let bearer = format!("Bearer {TOKEN}");
    let big = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\",\"pad\":\"{}\"}}",
        "a".repeat(1 << 20)
    );
    let headers = [
        ("Host", host.as_str()),
        ("Authorization", bearer.as_str()),
        ("Content-Type", "application/json"),
        ("MCP-Protocol-Version", "2025-06-18"),
    ];
    // Refused by its length, before its body is read.
    let head_only = request("POST", "/mcp", &headers, None);
    let mut head = String::from_utf8(head_only).unwrap();
    head.truncate(head.len() - 2);
    head.push_str(&format!("Content-Length: {}\r\n\r\n", big.len()));
    let mut conn = TcpStream::connect(engined.address).unwrap();
    conn.write_all(head.as_bytes()).unwrap();
    let answer = read_answer(&mut conn);
    assert_eq!(answer.status, 413, "{}", answer.whole());
    // Nested deeper than 16.
    let deep = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\",\"params\":{{\"a\":{}1{}}}}}",
        "[".repeat(20),
        "]".repeat(20)
    );
    let answer = send(
        engined.address,
        &request("POST", "/mcp", &headers, Some(deep.as_bytes())),
    );
    assert_eq!(answer.status, 400, "{}", answer.whole());
    assert!(answer.text().contains("16 deep"));
    // Not JSON; another type.
    let answer = send(
        engined.address,
        &request("POST", "/mcp", &headers, Some(b"{not json")),
    );
    assert_eq!(answer.status, 400);
    let mut typed = headers.to_vec();
    typed[2] = ("Content-Type", "text/plain");
    let answer = send(
        engined.address,
        &request("POST", "/mcp", &typed, Some(b"{}")),
    );
    assert_eq!(answer.status, 415);
}

#[test]
fn a_fifth_new_run_is_answered_429_and_a_repeat_run_routes_and_downloads_are_still_answered() {
    let engined = Engined::start("eg-fifth", |_| {});
    let picture = send(
        engined.address,
        &upload_bytes(&engined, "image/png", &small_png(), ""),
    );
    let picture = picture.json()["image"].as_str().unwrap().to_owned();
    let streams = four_streams(&engined);
    // A fifth new run, with a request id and without: 429, and no run.
    for arguments in [json!({"request_id": fresh_id()}), json!({})] {
        let answer = engined.mcp(&call("waits", arguments));
        assert_eq!(answer.status, 429, "{}", answer.whole());
        assert!(answer.header("retry-after").is_some());
    }
    assert_eq!(engined.ran("waits"), 4);
    // A repeat of the first takes its stream over.
    let (first, id) = &streams[0];
    let run = first.run();
    let repeat = engined.open_call("waits", json!({"request_id": id}));
    assert_eq!(repeat.status, 200, "{}", repeat.head);
    assert_eq!(repeat.run(), run);
    // The run routes and downloads.
    let status = engined.api("GET", &format!("/v1/runs/{run}"), None);
    assert_eq!(status.status, 200, "{}", status.whole());
    assert_eq!(status.json()["state"], "running");
    let download = engined.api("GET", &format!("/v1/artifacts/{picture}"), None);
    assert_eq!(download.status, 200, "{}", download.whole());
    assert_eq!(download.body, small_png());
    let cancel = engined.api("POST", &format!("/v1/runs/{run}/cancel"), None);
    assert_eq!(cancel.status, 202, "{}", cancel.whole());
    let ended = events(&repeat.rest());
    let result = ended.last().unwrap();
    assert_eq!(result["result"]["_meta"]["whizbang/run"], run.as_str());
    assert_eq!(engined.ran("waits"), 4);
}

#[test]
fn a_third_upload_is_answered_429_after_its_headers() {
    // A stalled body is held to its time from the connection's acceptance: longer here, so
    // that the two hold their places for as long as a busy runner takes to send the third.
    let engined = Engined::start("eg-third-upload", |settings| {
        settings.times.head = Duration::from_secs(20);
    });
    let host = engined.host_header();
    let bearer = format!("Bearer {TOKEN}");
    let head = format!(
        "POST /v1/uploads HTTP/1.1\r\nHost: {host}\r\nAuthorization: {bearer}\r\n\
         Content-Type: image/png\r\nContent-Length: 1000\r\n\r\n"
    );
    // Two uploads hold their handlers, their bodies stalled after the headers.
    let mut stalled = Vec::new();
    for _ in 0..2 {
        let mut conn = TcpStream::connect(engined.address).unwrap();
        conn.write_all(head.as_bytes()).unwrap();
        stalled.push(conn);
    }
    // The third is refused as soon as its headers are read. The two may still be on their
    // readers for a moment, so it is sent whole, which never stalls, until they hold their
    // places: then it is refused without its body being read.
    let whole = small_png();
    let limit = Instant::now() + Duration::from_secs(10);
    let answer = loop {
        let answer = send(
            engined.address,
            &upload_bytes(&engined, "image/png", &whole, ""),
        );
        if answer.status == 429 || Instant::now() > limit {
            break answer;
        }
        assert_eq!(answer.status, 201, "{}", answer.whole());
    };
    assert_eq!(answer.status, 429, "{}", answer.whole());
    assert!(answer.header("retry-after").is_some());
    // Now that the two hold their places, a third sent as its headers alone is refused at
    // once: nothing of its body is waited for.
    let started = Instant::now();
    let mut conn = TcpStream::connect(engined.address).unwrap();
    conn.write_all(head.as_bytes()).unwrap();
    let answer = read_answer(&mut conn);
    assert_eq!(answer.status, 429, "{}", answer.whole());
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
    // A JSON request is still answered.
    assert_eq!(engined.mcp(&ping()).status, 200);
    drop(stalled);
}

#[test]
fn stalled_streams_and_uploads_and_trickling_clients_leave_the_run_routes_answered() {
    let engined = Engined::start("eg-stalled", |_| {});
    let streams = four_streams(&engined);
    let run = streams[0].0.run();
    let host = engined.host_header();
    let bearer = format!("Bearer {TOKEN}");
    let upload = format!(
        "POST /v1/uploads HTTP/1.1\r\nHost: {host}\r\nAuthorization: {bearer}\r\n\
         Content-Type: image/png\r\nContent-Length: 100000\r\n\r\npartial"
    );
    let mut held = Vec::new();
    for _ in 0..2 {
        let mut conn = TcpStream::connect(engined.address).unwrap();
        conn.write_all(upload.as_bytes()).unwrap();
        held.push(conn);
    }
    // Ten clients that trickle their headers, a few bytes and no more.
    for _ in 0..10 {
        let mut conn = TcpStream::connect(engined.address).unwrap();
        conn.write_all(b"POST /mcp HTTP/1.1\r\nHo").unwrap();
        held.push(conn);
    }
    for _ in 0..3 {
        let started = Instant::now();
        let status = engined.api("GET", &format!("/v1/runs/{run}"), None);
        assert_eq!(status.status, 200, "{}", status.whole());
        assert!(started.elapsed() < Duration::from_secs(2));
    }
    let cancel = engined.api("POST", &format!("/v1/runs/{run}/cancel"), None);
    assert_eq!(cancel.status, 202, "{}", cancel.whole());
    drop(held);
}

#[test]
fn with_every_reader_held_by_unauthenticated_incomplete_headers_a_whole_status_and_a_cancel_are_answered_within_2_seconds_of_their_acceptance()
 {
    let engined = Engined::start("eg-readers", |_| {});
    let streams = four_streams(&engined);
    let run = streams[1].0.run();
    // Sixteen connections, each a reader's, none of them finishing its headers.
    let mut held = Vec::new();
    for _ in 0..16 {
        let mut conn = TcpStream::connect(engined.address).unwrap();
        conn.write_all(b"GET /v1/runs/x HTTP/1.1\r\nHost: 127")
            .unwrap();
        held.push(conn);
    }
    for (method, path, expected) in [
        ("GET", format!("/v1/runs/{run}"), 200),
        ("POST", format!("/v1/runs/{run}/cancel"), 202),
    ] {
        let started = Instant::now();
        let answer = engined.api(method, &path, None);
        assert_eq!(answer.status, expected, "{}", answer.whole());
        let took = started.elapsed();
        assert!(took < Duration::from_secs(2), "{took:?}");
    }
    drop(held);
}

#[test]
fn with_four_streams_open_a_repeat_of_one_takes_over_its_stream_at_once_and_dispatches_and_spends_nothing_more()
 {
    let engined = Engined::start("eg-takeover", |_| {});
    let mut streams = four_streams(&engined);
    let (older, id) = streams.remove(0);
    let run = older.run();
    let started = Instant::now();
    let repeat = engined.open_call("waits", json!({"request_id": id}));
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(repeat.status, 200, "{}", repeat.head);
    assert_eq!(repeat.run(), run);
    // The older stream is closed, unanswered.
    let older = older.rest();
    assert!(
        !events(&older)
            .iter()
            .any(|event| event.get("result").is_some()),
        "{older}"
    );
    // Nothing was started again.
    assert_eq!(engined.ran("waits"), 4);
    let cancel = engined.api("POST", &format!("/v1/runs/{run}/cancel"), None);
    assert_eq!(cancel.status, 202);
    let followed = repeat.rest();
    let result = events(&followed).pop().unwrap();
    assert_eq!(result["id"], 1);
    assert_eq!(result["result"]["_meta"]["whizbang/run"], run.as_str());
    assert_eq!(engined.ran("waits"), 4);
}

#[test]
fn a_repeat_sent_as_headers_then_body_with_four_streams_open_and_calls_stalled_in_their_bodies_is_reattached_and_status_and_cancel_answered()
 {
    let engined = Engined::start("eg-split-repeat", |_| {});
    let streams = four_streams(&engined);
    let (first, id) = &streams[2];
    let run = first.run();
    let host = engined.host_header();
    let bearer = format!("Bearer {TOKEN}");
    // Eight calls stalled in their bodies.
    let mut held = Vec::new();
    for _ in 0..8 {
        let mut conn = TcpStream::connect(engined.address).unwrap();
        let head = format!(
            "POST /mcp HTTP/1.1\r\nHost: {host}\r\nAuthorization: {bearer}\r\n\
             Content-Type: application/json\r\nMCP-Protocol-Version: 2025-06-18\r\n\
             Content-Length: 500\r\n\r\n{{\"jsonrpc\""
        );
        conn.write_all(head.as_bytes()).unwrap();
        held.push(conn);
    }
    // The repeat: its headers, then its body.
    let message = json!({"jsonrpc": "2.0", "id": "again", "method": "tools/call",
                         "params": {"name": "waits", "arguments": {"request_id": id}}})
    .to_string();
    let head = format!(
        "POST /mcp HTTP/1.1\r\nHost: {host}\r\nAuthorization: {bearer}\r\n\
         Content-Type: application/json\r\nMCP-Protocol-Version: 2025-06-18\r\n\
         Content-Length: {}\r\n\r\n",
        message.len()
    );
    let mut conn = TcpStream::connect(engined.address).unwrap();
    conn.write_all(head.as_bytes()).unwrap();
    conn.flush().unwrap();
    conn.write_all(message.as_bytes()).unwrap();
    let mut bytes = Vec::new();
    let mut piece = [0u8; 1024];
    conn.set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    while !bytes.windows(4).any(|w| w == b"\r\n\r\n") {
        let read = conn.read(&mut piece).unwrap();
        assert!(read > 0, "closed");
        bytes.extend_from_slice(&piece[..read]);
    }
    let answer = crate::parse(&bytes);
    assert_eq!(answer.status, 200, "{}", answer.whole());
    assert_eq!(answer.header("whizbang-run"), Some(run.clone()));
    let status = engined.api("GET", &format!("/v1/runs/{run}"), None);
    assert_eq!(status.status, 200);
    let cancel = engined.api("POST", &format!("/v1/runs/{run}/cancel"), None);
    assert_eq!(cancel.status, 202);
    assert_eq!(engined.ran("waits"), 4);
    drop(held);
}

#[test]
fn a_repeat_of_an_ended_run_at_capacity_is_answered_as_one_body() {
    let engined = Engined::start("eg-ended-repeat", |_| {});
    let id = fresh_id();
    let first = engined.open_call("quick", json!({"request_id": id}));
    let run = first.run();
    let ended = events(&first.rest());
    assert_eq!(ended.last().unwrap()["result"]["isError"], false);
    let _streams = four_streams(&engined);
    let answer = engined.mcp(&call("quick", json!({"request_id": id})));
    assert_eq!(answer.status, 200, "{}", answer.whole());
    assert_eq!(
        answer.header("content-type").as_deref(),
        Some("application/json")
    );
    assert_eq!(answer.header("whizbang-run"), Some(run.clone()));
    let result = &answer.json()["result"];
    assert_eq!(result["_meta"]["whizbang/run"], run.as_str());
    assert_eq!(result["isError"], false, "{}", answer.text());
    assert_eq!(engined.ran("quick"), 1);
}

#[test]
fn the_whole_engine_saturated_holds_no_more_than_its_32_threads() {
    let engined = Engined::start("eg-threads", |_| {});
    // One engine is thirty threads: the acceptor, sixteen readers, twelve handlers and
    // the canceller. Saturated, it makes no more.
    assert_eq!(threads::live(), 30);
    let _streams = four_streams(&engined);
    let host = engined.host_header();
    let bearer = format!("Bearer {TOKEN}");
    let mut held = Vec::new();
    for _ in 0..2 {
        let mut conn = TcpStream::connect(engined.address).unwrap();
        let head = format!(
            "POST /v1/uploads HTTP/1.1\r\nHost: {host}\r\nAuthorization: {bearer}\r\n\
             Content-Type: image/png\r\nContent-Length: 1000\r\n\r\n"
        );
        conn.write_all(head.as_bytes()).unwrap();
        held.push(conn);
    }
    for _ in 0..20 {
        let mut conn = TcpStream::connect(engined.address).unwrap();
        conn.write_all(b"GET /v1/runs/x HTTP/1.1\r\n").unwrap();
        held.push(conn);
    }
    assert_eq!(engined.api("GET", "/v1/runs/x", None).status, 404);
    assert_eq!(threads::live(), 30);
    assert!(threads::live() <= threads::THREADS_LIMIT);
    // A second engine in the same process is refused, and leaves nothing behind.
    let other = whizbang_serve::engine::Host::start(
        engined.executor.clone(),
        std::sync::Arc::new(NoTools),
        whizbang_serve::engine::Settings::new(SocketAddr::from(([127, 0, 0, 1], 0))),
        Secret::new(TOKEN).unwrap(),
    );
    assert!(other.is_err());
    assert_eq!(threads::live(), 30);
    drop(held);
}

/// A door with nothing in it.
struct NoTools;

impl whizbang_serve::Door for NoTools {
    fn tools(&self) -> Vec<whizbang_serve::Tool> {
        Vec::new()
    }
    fn call(
        &self,
        _: &str,
        _: &serde_json::Map<String, Json>,
    ) -> Result<(Call, Option<String>), String> {
        Err("none".to_owned())
    }
    fn said(
        &self,
        _: &str,
        _: &RunId,
        _: &whizbang_core::contract::Outcome,
    ) -> whizbang_serve::Said {
        whizbang_serve::Said {
            text: String::new(),
            error: true,
        }
    }
}

impl whizbang_serve::engine::Hosted for NoTools {
    fn upload(&self, _: &[u8], _: u64) -> Result<Json, whizbang_serve::engine::Refused> {
        Err(whizbang_serve::engine::Refused::new(501, "none"))
    }
    fn mask(&self, _: &[u8], _: &str, _: u64) -> Result<Json, whizbang_serve::engine::Refused> {
        Err(whizbang_serve::engine::Refused::new(501, "none"))
    }
    fn image_size(&self, _: &str) -> Option<(u32, u32)> {
        None
    }
    fn artifact(
        &self,
        _: &str,
    ) -> Result<(&'static str, Vec<u8>), whizbang_serve::engine::Refused> {
        Err(whizbang_serve::engine::Refused::new(404, "none"))
    }
    fn admitted(
        &self,
        _: &whizbang_core::contract::RequestId,
    ) -> Option<(RunId, whizbang_core::identity::Digest)> {
        None
    }
}

#[test]
fn no_local_control_is_listed_callable_or_routed() {
    let engined = Engined::start("eg-local", |_| {});
    let listed = engined.mcp(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}));
    let names: Vec<String> = (listed.json()["result"]["tools"].as_array().unwrap().iter())
        .map(|tool| tool["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        names,
        ["quick", "waits", "heeds", "deaf", "held", "described"]
    );
    for tool in [
        "models_get",
        "models_remove",
        "spend_reset",
        "store_prune",
        "config",
        "setup",
    ] {
        let answer = engined.mcp(&call(tool, json!({})));
        assert_eq!(answer.status, 200);
        assert_eq!(answer.json()["error"]["code"], -32602, "{tool}");
    }
    for (method, path) in [
        ("POST", "/v1/models/sam3"),
        ("DELETE", "/v1/models/sam3"),
        ("POST", "/v1/spend/reset"),
        ("POST", "/v1/store/prune"),
        ("GET", "/api/hello"),
        ("POST", "/api/quit"),
        ("GET", "/"),
        ("GET", "/mcp"),
    ] {
        let answer = engined.api(method, path, None);
        assert!(
            answer.status == 404 || answer.status == 405,
            "{method} {path}: {}",
            answer.whole()
        );
    }
    assert_eq!(engined.api("GET", "/mcp", None).status, 405);
}

#[test]
fn a_request_with_no_protocol_version_another_one_or_a_batch_is_refused_400() {
    let engined = Engined::start("eg-version", |_| {});
    let host = engined.host_header();
    let bearer = format!("Bearer {TOKEN}");
    let body = ping().to_string();
    for version in [None, Some("2025-03-26"), Some("2024-11-05"), Some("")] {
        let mut headers = vec![
            ("Host", host.as_str()),
            ("Authorization", bearer.as_str()),
            ("Content-Type", "application/json"),
        ];
        if let Some(version) = version {
            headers.push(("MCP-Protocol-Version", version));
        }
        let answer = send(
            engined.address,
            &request("POST", "/mcp", &headers, Some(body.as_bytes())),
        );
        assert_eq!(answer.status, 400, "{version:?}: {}", answer.whole());
    }
    let batch = json!([ping(), ping()]);
    let answer = engined.mcp(&batch);
    assert_eq!(answer.status, 400, "{}", answer.whole());
    assert!(answer.text().contains("batch"));
    // initialize needs none, and is answered with the one revision offered.
    let headers = [
        ("Host", host.as_str()),
        ("Authorization", bearer.as_str()),
        ("Content-Type", "application/json"),
    ];
    let initialize = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                            "params": {"protocolVersion": "2024-11-05"}})
    .to_string();
    let answer = send(
        engined.address,
        &request("POST", "/mcp", &headers, Some(initialize.as_bytes())),
    );
    assert_eq!(answer.status, 200, "{}", answer.whole());
    assert_eq!(answer.json()["result"]["protocolVersion"], "2025-06-18");
    assert!(answer.header("mcp-session-id").is_none());
}

#[test]
fn a_cancel_of_a_run_this_server_did_not_start_is_answered_409() {
    let engined = Engined::start("eg-409", |_| {});
    // A run of this process that the server did not start, going.
    let elsewhere = engined
        .executor
        .start(&Request::new(Call::new("waits")))
        .unwrap();
    let run = match elsewhere {
        whizbang_core::contract::Answer::Started(run) => run,
        other => panic!("{other:?}"),
    };
    let answer = engined.api("POST", &format!("/v1/runs/{}/cancel", run.as_str()), None);
    assert_eq!(answer.status, 409, "{}", answer.whole());
    // Nothing was done to it: it is cancelled by whoever started it.
    let _ = engined.executor.cancel(&run);
    let _ = engined.executor.wait(&run);
    // Once it has ended, any cancel is answered with its end, which it does not change.
    let answer = engined.api("POST", &format!("/v1/runs/{}/cancel", run.as_str()), None);
    assert_eq!(answer.status, 200, "{}", answer.whole());
    assert_eq!(answer.json()["outcome"], "cancelled");
    // One it started, ended: its end.
    let mine = engined.open_call("quick", json!({}));
    let run = mine.run();
    let _ = mine.rest();
    let answer = engined.api("POST", &format!("/v1/runs/{run}/cancel"), None);
    assert_eq!(answer.status, 200, "{}", answer.whole());
    assert_eq!(answer.json()["outcome"], "completed");
    // One the store does not hold.
    let answer = engined.api("POST", "/v1/runs/20260101T000000Z-1/cancel", None);
    assert_eq!(answer.status, 404, "{}", answer.whole());
}

#[test]
fn a_run_held_in_its_final_publication_leaves_repeated_cancels_another_runs_status_and_deadline_and_a_signals_grace_answered()
 {
    // The stated time is long enough that the other run's own ending cannot come before
    // the held run's publication takes the gate: the gate holds whichever publication
    // comes next after the held run's action.
    let mut engined = Engined::start("eg-held", |settings| {
        settings.wait = Duration::from_secs(3);
    });
    // Another run first, which waits to be stopped: its stated time comes while the held
    // run's publication waits.
    // It heeds its cancellation alone, not its deadline: whatever stops it is the host's
    // watch of its stated time.
    let other = engined.open_call("heeds", json!({}));
    let other_run = other.run();
    engined.until_state(&other_run, "running");
    let held = engined.open_call("held", json!({}));
    let run = held.run();
    engined.gate.entered();
    // The canceller waits in the executor's cancel; no handler does.
    for _ in 0..20 {
        let started = Instant::now();
        let answer = engined.api("POST", &format!("/v1/runs/{run}/cancel"), None);
        assert_eq!(answer.status, 202, "{}", answer.whole());
        assert!(started.elapsed() < Duration::from_secs(2));
    }
    // The other run's status is answered, and its stated time stops it: it says stopping,
    // or, where its action saw the cancellation and its end became visible first, ended.
    let limit = Instant::now() + Duration::from_secs(10);
    let state = loop {
        let polled = engined.api("GET", &format!("/v1/runs/{other_run}"), None);
        assert_eq!(polled.status, 200, "{}", polled.whole());
        let state = polled.json()["state"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        if state != "running" || Instant::now() > limit {
            break state;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(state == "stopping" || state == "ended", "{state}");
    // A signal's stop ends at the end of its grace, with the held run left (the other,
    // cancelled, may have ended by then or not).
    let started = Instant::now();
    let stopped = engined.stop();
    let took = started.elapsed();
    assert!((1..=2).contains(&stopped.left), "{stopped:?}");
    assert!(
        took >= Duration::from_millis(350) && took < Duration::from_secs(5),
        "{took:?}"
    );
    engined.gate.release();
    drop((other, held));
}

#[test]
fn the_token_is_in_no_answer_log_event_record_or_error() {
    let engined = Engined::start("eg-token", |_| {});
    let mut said = String::new();
    let opened = engined.open_call("quick", json!({"request_id": fresh_id()}));
    said.push_str(&opened.head);
    said.push_str(&opened.rest());
    for answer in [
        engined.mcp(&ping()),
        engined.mcp(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"})),
        engined.mcp(&call("quick", json!({"request_id": fresh_id()}))),
        engined.mcp(&call("nothing", json!({}))),
        engined.api("GET", "/v1/runs/nothing", None),
        send(
            engined.address,
            &request(
                "POST",
                "/mcp",
                &[
                    ("Host", &engined.host_header()),
                    ("Authorization", "Bearer wrong"),
                ],
                None,
            ),
        ),
    ] {
        said.push_str(&answer.whole());
    }
    assert!(!said.contains(TOKEN), "{said}");
    // Nothing the store holds names it: records, journals, events.
    let mut stack = vec![engined.scratch.path().to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let bytes = std::fs::read(&path).unwrap_or_default();
                assert!(
                    !bytes.windows(TOKEN.len()).any(|w| w == TOKEN.as_bytes()),
                    "{}",
                    path.display()
                );
            }
        }
    }
    // The token's own type prints nothing.
    let secret = Secret::new(TOKEN).unwrap();
    assert!(!format!("{secret:?}{secret}").contains(TOKEN));
}

#[test]
fn calls_are_held_to_the_stream_handlers_and_at_their_most_each_run_has_its_stream() {
    // More calls than stream handlers would leave a run admitted with nobody to follow it:
    // refused at start, and nothing is left running.
    let executor = Engined::start("eg-calls-refused", |_| {}).executor.clone();
    let before = threads::live();
    for calls in [0, 5, 16] {
        let mut settings =
            whizbang_serve::engine::Settings::new(SocketAddr::from(([127, 0, 0, 1], 0)));
        settings.calls = calls;
        let refused = whizbang_serve::engine::Host::start(
            executor.clone(),
            std::sync::Arc::new(NoTools),
            settings,
            Secret::new(TOKEN).unwrap(),
        );
        assert!(refused.is_err(), "{calls}");
    }
    // Refused before a thread is made.
    assert!(threads::live() <= before);
    // At its most, four: each run's stream is answered at once with its head, and a fifth
    // is refused.
    let engined = Engined::start("eg-calls-most", |settings| {
        settings.calls = whizbang_serve::engine::STREAMS;
    });
    let started = Instant::now();
    let streams = four_streams(&engined);
    assert!(started.elapsed() < Duration::from_secs(5));
    let runs: std::collections::BTreeSet<String> =
        streams.iter().map(|(opened, _)| opened.run()).collect();
    assert_eq!(runs.len(), 4);
    let fifth = engined.mcp(&call("waits", json!({})));
    assert_eq!(fifth.status, 429, "{}", fifth.whole());
}

#[test]
fn a_repeat_while_its_run_is_being_started_follows_it_and_is_never_told_it_was_given_up() {
    let engined = Engined::start("eg-repeat-starting", |_| {});
    let id = fresh_id();
    let message = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                         "params": {"name": "waits", "arguments": {"request_id": id}}});
    let bytes = engined.bytes("POST", "/mcp", Some(&message));
    // The start's admission waits at the gate, before it is on the disk.
    engined.hold_next_publication();
    let mut first = TcpStream::connect(engined.address).unwrap();
    first.write_all(&bytes).unwrap();
    engined.gate.entered();
    // The same request again, while the first is being started.
    let mut second = TcpStream::connect(engined.address).unwrap();
    second.write_all(&bytes).unwrap();
    engined.gate.release();
    let first = Opened::read_head(first);
    let second = Opened::read_head(second);
    assert_eq!(first.status, 200, "{}", first.head);
    assert_eq!(second.status, 200, "{}", second.head);
    let run = first.run();
    assert_eq!(second.run(), run);
    engined.until_state(&run, "running");
    assert_eq!(engined.ran("waits"), 1);
    let cancel = engined.api("POST", &format!("/v1/runs/{run}/cancel"), None);
    assert_eq!(cancel.status, 202);
    let said = format!("{}{}", first.rest(), second.rest());
    assert!(!said.contains("given up"), "{said}");
    assert!(
        said.contains(&format!("\"whizbang/run\":\"{run}\"")),
        "{said}"
    );
    assert_eq!(engined.ran("waits"), 1);
}
