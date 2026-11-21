//! `play_guard::…`: the guard of the playground's API (`SPEC.md` section 10; plan m5,
//! "The playground API", the negative tests; ADR 0041, "Who may call"), each check shown
//! refusing on a real loopback socket.

use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream, UdpSocket};
use std::time::{Duration, Instant};

use serde_json::json;
use whizbang_serve::play::Token;

use crate::{Answer, Running, read_answer, request, send};

/// The guard's headers, which every answer carries, and the one it never carries.
fn guarded_headers(answer: &Answer) {
    assert_eq!(answer.header("cache-control").as_deref(), Some("no-store"));
    assert_eq!(
        answer.header("x-content-type-options").as_deref(),
        Some("nosniff")
    );
    assert_eq!(
        answer.header("referrer-policy").as_deref(),
        Some("no-referrer")
    );
    assert_eq!(
        answer.header("cross-origin-resource-policy").as_deref(),
        Some("same-origin")
    );
    assert!(
        !answer.head.to_ascii_lowercase().contains("access-control-"),
        "{}",
        answer.head
    );
}

fn hello_with(running: &Running, headers: &[(&str, &str)]) -> Answer {
    send(
        running.address,
        &request("GET", "/api/hello", headers, None),
    )
}

#[test]
fn an_api_call_without_the_token_is_refused_401_and_names_nothing() {
    let running = Running::start("guard-no-token", |_| {});
    let host = running.host_header();
    let store = running.scratch.path().to_string_lossy().into_owned();
    for headers in [
        vec![("Host", host.as_str())],
        vec![("Host", host.as_str()), ("Authorization", "Basic abc")],
        vec![("Host", host.as_str()), ("Authorization", "Bearer ")],
        vec![("Host", host.as_str()), ("Authorization", "bearer x")],
    ] {
        let answer = hello_with(&running, &headers);
        assert_eq!(answer.status, 401, "{}", answer.whole());
        let said = answer.whole();
        assert!(!said.contains(&store), "{said}");
        assert!(!said.contains(env!("CARGO_PKG_VERSION")), "{said}");
        assert!(
            answer.json()["refused"]
                .as_str()
                .unwrap()
                .contains("whizbang play")
        );
        guarded_headers(&answer);
    }
    // Every route under /api/ is behind it, whatever the method.
    for (method, path) in [
        ("POST", "/api/quit"),
        ("GET", "/api/runs?open=1"),
        ("GET", "/api/nothing-here"),
    ] {
        let answer = send(
            running.address,
            &request(
                method,
                path,
                &[("Host", &host), ("Origin", &running.origin())],
                None,
            ),
        );
        assert_eq!(answer.status, 401, "{method} {path}");
    }
    // With the token it answers.
    assert_eq!(running.api("GET", "/api/hello", None).status, 200);
}

#[test]
fn an_api_call_with_a_token_of_an_earlier_launch_is_refused_401() {
    let running = Running::start("guard-old-token", |_| {});
    let host = running.host_header();
    // Another launch's token; and this launch's with its last digit changed, which a
    // comparison that stops early would take longest over.
    let earlier = Token::new().unwrap();
    let mut close = running.token.hex();
    let last = close.pop().unwrap();
    close.push(if last == '0' { '1' } else { '0' });
    for presented in [earlier.hex(), close, running.token.hex().to_uppercase()] {
        let bearer = format!("Bearer {presented}");
        let answer = hello_with(&running, &[("Host", &host), ("Authorization", &bearer)]);
        assert_eq!(answer.status, 401, "{}", answer.whole());
    }
    assert_eq!(running.api("GET", "/api/hello", None).status, 200);
}

#[test]
fn a_request_whose_host_is_not_loopback_at_the_port_is_refused_421() {
    let running = Running::start("guard-host", |_| {});
    let port = running.address.port();
    let bearer = format!("Bearer {}", running.token.hex());
    let other = format!("127.0.0.1:{}", port.wrapping_add(1));
    let rebound = format!("attacker.example:{port}");
    let no_port = "127.0.0.1".to_owned();
    let v6 = format!("[::1]:{port}");
    for host in [
        rebound.as_str(),
        other.as_str(),
        no_port.as_str(),
        v6.as_str(),
    ] {
        let answer = hello_with(&running, &[("Host", host), ("Authorization", &bearer)]);
        assert_eq!(answer.status, 421, "{host}: {}", answer.whole());
        guarded_headers(&answer);
    }
    // No Host at all; and the page itself, which needs no token, is behind it too.
    let answer = hello_with(&running, &[("Authorization", &bearer)]);
    assert_eq!(answer.status, 421);
    let page = send(
        running.address,
        &request("GET", "/", &[("Host", &rebound)], None),
    );
    assert_eq!(page.status, 421);
    // Both names the host answers to.
    let localhost = format!("localhost:{port}");
    let answer = hello_with(
        &running,
        &[("Host", &localhost), ("Authorization", &bearer)],
    );
    assert_eq!(answer.status, 200, "{}", answer.whole());
}

#[test]
fn a_request_from_another_origin_is_refused_403_and_carries_no_allow_origin_header() {
    let running = Running::start("guard-origin", |_| {});
    let host = running.host_header();
    let bearer = format!("Bearer {}", running.token.hex());
    let port = running.address.port();
    let elsewhere = format!("http://127.0.0.1:{}", port.wrapping_add(1));
    let secure = format!("https://127.0.0.1:{port}");
    for origin in [
        "http://attacker.example",
        elsewhere.as_str(),
        secure.as_str(),
        "null",
    ] {
        let answer = hello_with(
            &running,
            &[
                ("Host", &host),
                ("Authorization", &bearer),
                ("Origin", origin),
            ],
        );
        assert_eq!(answer.status, 403, "{origin}: {}", answer.whole());
        guarded_headers(&answer);
        let quit = send(
            running.address,
            &request(
                "POST",
                "/api/quit",
                &[
                    ("Host", &host),
                    ("Authorization", &bearer),
                    ("Origin", origin),
                ],
                None,
            ),
        );
        assert_eq!(quit.status, 403, "{origin}");
    }
    // The host's own origins are taken, and the answer allows no other.
    for origin in [running.origin(), format!("http://localhost:{port}")] {
        let answer = hello_with(
            &running,
            &[
                ("Host", &host),
                ("Authorization", &bearer),
                ("Origin", &origin),
            ],
        );
        assert_eq!(answer.status, 200, "{origin}");
        guarded_headers(&answer);
    }
}

#[test]
fn a_change_without_an_origin_is_refused_403() {
    let running = Running::start("guard-no-origin", |_| {});
    let host = running.host_header();
    let bearer = format!("Bearer {}", running.token.hex());
    let body = br#"{"action":"quick","arguments":{}}"#;
    for (method, path, body) in [
        ("POST", "/api/quit", None),
        ("POST", "/api/prepare", Some(&body[..])),
        ("POST", "/api/runs", Some(&body[..])),
    ] {
        let mut headers = vec![("Host", host.as_str()), ("Authorization", bearer.as_str())];
        if body.is_some() {
            headers.push(("Content-Type", "application/json"));
        }
        let answer = send(running.address, &request(method, path, &headers, body));
        assert_eq!(answer.status, 403, "{path}: {}", answer.whole());
    }
    // Nothing stopped: the host still answers.
    assert_eq!(running.api("GET", "/api/hello", None).status, 200);
}

#[test]
fn a_cross_site_fetch_metadata_header_is_refused_403() {
    let running = Running::start("guard-fetch-site", |_| {});
    let host = running.host_header();
    let bearer = format!("Bearer {}", running.token.hex());
    for site in ["cross-site", "same-site", "something-else"] {
        let api = hello_with(
            &running,
            &[
                ("Host", &host),
                ("Authorization", &bearer),
                ("Sec-Fetch-Site", site),
            ],
        );
        assert_eq!(api.status, 403, "{site}");
        let page = send(
            running.address,
            &request(
                "GET",
                "/",
                &[("Host", &host), ("Sec-Fetch-Site", site)],
                None,
            ),
        );
        assert_eq!(page.status, 403, "{site}");
    }
    for site in ["same-origin", "none"] {
        let api = hello_with(
            &running,
            &[
                ("Host", &host),
                ("Authorization", &bearer),
                ("Sec-Fetch-Site", site),
            ],
        );
        assert_eq!(api.status, 200, "{site}");
    }
}

/// The headers of an upload of `length` bytes, as the page sends one.
fn upload_head(running: &Running, length: u64, content_type: &str) -> Vec<u8> {
    format!(
        "POST /api/pictures?name=a.png HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\n\
         Origin: {}\r\nContent-Type: {content_type}\r\nContent-Length: {length}\r\n\r\n",
        running.host_header(),
        running.token.hex(),
        running.origin()
    )
    .into_bytes()
}

#[test]
fn a_picture_over_64_mib_is_refused_413_before_it_is_read_whole() {
    let running = Running::start("guard-big-picture", |_| {});
    // The headers alone, saying one byte over 64 MiB: answered without a byte of the
    // body sent.
    let started = Instant::now();
    let mut stream = TcpStream::connect(running.address).unwrap();
    stream
        .write_all(&upload_head(&running, 64 * 1024 * 1024 + 1, "image/png"))
        .unwrap();
    let answer = read_answer(&mut stream);
    assert_eq!(answer.status, 413, "{}", answer.whole());
    assert!(answer.text().contains("64 MiB"), "{}", answer.text());
    assert!(started.elapsed() < Duration::from_secs(3));
    // A body of another type is refused as such.
    let mut stream = TcpStream::connect(running.address).unwrap();
    stream
        .write_all(&upload_head(&running, 10, "text/plain"))
        .unwrap();
    assert_eq!(read_answer(&mut stream).status, 415);
}

/// The CRC-32 of PNG's chunks.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                0xedb8_8320 ^ (crc >> 1)
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn chunk(png: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    png.extend_from_slice(&u32::try_from(data.len()).unwrap().to_be_bytes());
    let mut typed = kind.to_vec();
    typed.extend_from_slice(data);
    png.extend_from_slice(&typed);
    png.extend_from_slice(&crc32(&typed).to_be_bytes());
}

/// A PNG of a few bytes whose header says it is `width` by `height` grey pixels.
fn png_saying(width: u32, height: u32) -> Vec<u8> {
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::new();
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 0, 0, 0, 0]);
    chunk(&mut png, b"IHDR", &header);
    chunk(
        &mut png,
        b"IDAT",
        &[0x78, 0x9c, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01],
    );
    chunk(&mut png, b"IEND", &[]);
    png
}

#[test]
fn a_picture_over_the_pixel_limit_is_refused_413_naming_the_limit() {
    let running = Running::start("guard-pixels", |_| {});
    let png = png_saying(10_000, 10_000);
    let mut bytes = upload_head(&running, png.len() as u64, "image/png");
    bytes.extend_from_slice(&png);
    let answer = send(running.address, &bytes);
    assert_eq!(answer.status, 413, "{}", answer.whole());
    assert!(answer.text().contains("50000000"), "{}", answer.text());
    // Bytes that are no picture are refused as such, and nothing is said of them.
    let mut bytes = upload_head(&running, 5, "image/png");
    bytes.extend_from_slice(b"hello");
    let answer = send(running.address, &bytes);
    assert_eq!(answer.status, 415, "{}", answer.whole());
    assert!(!answer.text().contains("hello"));
}

#[test]
fn a_json_body_over_1_mib_or_deeper_than_the_limit_is_refused() {
    let running = Running::start("guard-json", |_| {});
    let head = |length: usize, content_type: &str| {
        format!(
            "POST /api/prepare HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\nOrigin: {}\r\n\
             Content-Type: {content_type}\r\nContent-Length: {length}\r\n\r\n",
            running.host_header(),
            running.token.hex(),
            running.origin()
        )
        .into_bytes()
    };
    // Over 1 MiB, refused from its headers.
    let mut stream = TcpStream::connect(running.address).unwrap();
    stream
        .write_all(&head(1024 * 1024 + 1, "application/json"))
        .unwrap();
    let answer = read_answer(&mut stream);
    assert_eq!(answer.status, 413, "{}", answer.whole());
    // Deeper than the limit, though small.
    let deep = format!(
        "{{\"action\":\"quick\",\"arguments\":{{\"x\":{}{}}}}}",
        "[".repeat(40),
        "]".repeat(40)
    );
    let mut bytes = head(deep.len(), "application/json");
    bytes.extend_from_slice(deep.as_bytes());
    let answer = send(running.address, &bytes);
    assert_eq!(answer.status, 400, "{}", answer.whole());
    assert!(answer.text().contains("16 deep"), "{}", answer.text());
    // Not JSON, and JSON of another declared type.
    let mut bytes = head(3, "application/json");
    bytes.extend_from_slice(b"{{{");
    assert_eq!(send(running.address, &bytes).status, 400);
    let mut bytes = head(2, "text/plain");
    bytes.extend_from_slice(b"{}");
    assert_eq!(send(running.address, &bytes).status, 415);
}

#[test]
fn a_fifth_running_call_is_answered_429_and_polling_cancel_and_quit_are_still_answered() {
    let mut running = Running::start("guard-fifth", |_| {});
    let runs: Vec<String> = (0..4).map(|_| running.run("waits")).collect();
    let prepared = running.api(
        "POST",
        "/api/prepare",
        Some(&json!({"action": "waits", "arguments": {}})),
    );
    let handle = prepared.json()["handle"].as_str().unwrap().to_owned();
    let fifth = running.api(
        "POST",
        "/api/runs",
        Some(&json!({"action": "waits", "arguments": {}, "handle": handle})),
    );
    assert_eq!(fifth.status, 429, "{}", fifth.whole());
    assert!(fifth.header("retry-after").is_some());
    // Everything else is answered whatever is running.
    let polled = running.api("GET", &format!("/api/runs/{}", runs[0]), None);
    assert_eq!(polled.status, 200);
    assert_eq!(polled.json()["state"], "running");
    let open = running.api("GET", "/api/runs?open=1", None);
    assert_eq!(open.json()["runs"].as_array().unwrap().len(), 4);
    let stopped = running.api("POST", &format!("/api/runs/{}/cancel", runs[0]), None);
    assert_eq!(stopped.status, 202, "{}", stopped.whole());
    assert_eq!(stopped.json()["state"], "stopping");
    running.until_state(&runs[0], "ended");
    assert_eq!(running.api("GET", "/api/hello", None).status, 200);
    let quit = running.api("POST", "/api/quit", None);
    assert_eq!(quit.status, 202);
    assert_eq!(running.finish().left, 0);
}

#[test]
fn a_client_that_trickles_its_headers_or_reads_nothing_is_closed_at_its_deadline() {
    let running = Running::start("guard-trickle", |settings| {
        settings.deadlines.head = Duration::from_millis(300);
    });
    // Half a request line, and then nothing: closed at the deadline, answered nothing.
    let started = Instant::now();
    let mut stream = TcpStream::connect(running.address).unwrap();
    stream.write_all(b"GET /api/hel").unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut got = Vec::new();
    let read = stream.read_to_end(&mut got);
    let took = started.elapsed();
    assert!(read.is_ok() || got.is_empty(), "{read:?}");
    assert!(got.is_empty(), "{:?}", String::from_utf8_lossy(&got));
    assert!(took >= Duration::from_millis(250), "{took:?}");
    assert!(took < Duration::from_secs(5), "{took:?}");
    // Sixteen clients that send a whole request and read nothing hold no worker: a
    // seventeenth is answered.
    let mut silent = Vec::new();
    for _ in 0..16 {
        let mut stream = TcpStream::connect(running.address).unwrap();
        let host = running.host_header();
        stream
            .write_all(&request("GET", "/", &[("Host", &host)], None))
            .unwrap();
        silent.push(stream);
    }
    let started = Instant::now();
    assert_eq!(running.api("GET", "/api/hello", None).status, 200);
    assert!(started.elapsed() < Duration::from_secs(5));
    drop(silent);
}

#[test]
fn a_third_upload_is_answered_429_after_its_headers() {
    let running = Running::start("guard-third-upload", |settings| {
        settings.deadlines.body_grace = Duration::from_secs(20);
    });
    // Two uploads that send their headers and no body hold their places.
    let mut stalled = Vec::new();
    for _ in 0..2 {
        let mut stream = TcpStream::connect(running.address).unwrap();
        stream
            .write_all(&upload_head(&running, 1000, "image/png"))
            .unwrap();
        stalled.push(stream);
    }
    // A third, whole, until the two are counted: then it is answered 429 from its
    // headers. Before they are, it is taken, and its bytes are refused as no picture.
    let limit = Instant::now() + Duration::from_secs(10);
    loop {
        let mut bytes = upload_head(&running, 5, "image/png");
        bytes.extend_from_slice(b"hello");
        let answer = send(running.address, &bytes);
        if answer.status == 429 {
            assert!(answer.header("retry-after").is_some());
            break;
        }
        assert_eq!(answer.status, 415, "{}", answer.whole());
        assert!(Instant::now() < limit, "the two uploads were never counted");
    }
    // A poll is answered beside them.
    assert_eq!(running.api("GET", "/api/hello", None).status, 200);
    drop(stalled);
}

#[test]
fn two_stalled_uploads_and_ten_trickling_clients_leave_polling_cancel_and_quit_answered() {
    let mut running = Running::start("guard-stalled", |settings| {
        settings.deadlines.head = Duration::from_secs(20);
        settings.deadlines.body_grace = Duration::from_secs(20);
    });
    let run = running.run("waits");
    let mut held = Vec::new();
    for _ in 0..2 {
        let mut stream = TcpStream::connect(running.address).unwrap();
        stream
            .write_all(&upload_head(&running, 1000, "image/png"))
            .unwrap();
        held.push(stream);
    }
    for _ in 0..10 {
        let mut stream = TcpStream::connect(running.address).unwrap();
        stream.write_all(b"GET / HTTP/1.1\r\nHo").unwrap();
        held.push(stream);
    }
    let started = Instant::now();
    let polled = running.api("GET", &format!("/api/runs/{run}"), None);
    assert_eq!(polled.status, 200);
    let stopped = running.api("POST", &format!("/api/runs/{run}/cancel"), None);
    assert_eq!(stopped.status, 202);
    running.until_state(&run, "ended");
    assert_eq!(running.api("POST", "/api/quit", None).status, 202);
    assert!(started.elapsed() < Duration::from_secs(10));
    assert_eq!(running.finish().left, 0);
    drop(held);
}

#[test]
fn a_request_naming_a_path_in_any_argument_is_refused() {
    let running = Running::start("guard-path", |_| {});
    let host = running.host_header();
    // A field of the address that no route takes.
    let answer = running.api("GET", "/api/hello?path=%2Fetc%2Fhosts", None);
    assert_eq!(answer.status, 400, "{}", answer.whole());
    let answer = running.api("GET", "/api/runs?open=1&file=/etc/hosts", None);
    assert_eq!(answer.status, 400);
    // A field of a body that no route takes.
    let answer = running.api(
        "POST",
        "/api/prepare",
        Some(&json!({"action": "quick", "arguments": {}, "path": "/etc/hosts"})),
    );
    assert_eq!(answer.status, 400, "{}", answer.whole());
    // A path where an argument or an input is asked for: no action takes it.
    let answer = running.api(
        "POST",
        "/api/prepare",
        Some(&json!({"action": "quick", "arguments": {"image": "/etc/hosts"}})),
    );
    assert_eq!(answer.status, 422, "{}", answer.whole());
    assert!(!answer.text().contains("/etc/hosts"), "{}", answer.text());
    // A path to a file outside the interface's list, however it is written.
    for path in [
        "/../Cargo.toml",
        "/%2e%2e/Cargo.toml",
        "/etc/hosts",
        "//etc/hosts",
    ] {
        let answer = send(
            running.address,
            &request("GET", path, &[("Host", &host)], None),
        );
        assert_eq!(answer.status, 404, "{path}: {}", answer.whole());
    }
    let answer = send(
        running.address,
        &request(
            "GET",
            "http://127.0.0.1/etc/hosts",
            &[("Host", &host)],
            None,
        ),
    );
    assert_eq!(answer.status, 400);
}

#[test]
fn the_token_is_in_no_log_event_record_or_error() {
    let mut running = Running::start("guard-token-kept", |_| {});
    let token = running.token.hex();
    assert!(!format!("{:?} {}", running.token, running.token).contains(&token));
    let host = running.host_header();
    let mut answers = Vec::new();
    // Refusals, with the token and without, and every route's answer.
    let bearer = format!("Bearer {token}");
    answers.push(hello_with(
        &running,
        &[("Host", "attacker.example"), ("Authorization", &bearer)],
    ));
    answers.push(hello_with(
        &running,
        &[("Host", &host), ("Authorization", "Bearer 00")],
    ));
    answers.push(hello_with(
        &running,
        &[
            ("Host", &host),
            ("Authorization", &bearer),
            ("Origin", "http://a.example"),
        ],
    ));
    answers.push(running.api("GET", "/api/hello", None));
    answers.push(running.api("GET", "/api/nothing", None));
    answers.push(running.api("GET", "/api/runs/not-a-run", None));
    let run = running.run("quick");
    let ended = running.until_state(&run, "ended");
    assert_eq!(ended["outcome"], "completed");
    answers.push(running.api("GET", &format!("/api/runs/{run}"), None));
    answers.push(running.api("GET", "/api/runs?open=1", None));
    answers.push(send(
        running.address,
        &request("GET", "/", &[("Host", &host)], None),
    ));
    answers.push(running.api("POST", "/api/quit", None));
    for answer in &answers {
        assert!(!answer.whole().contains(&token), "{}", answer.whole());
    }
    running.finish();
    // Nothing the store holds says it: the run's record and journal among them.
    let mut files = vec![running.scratch.path().to_path_buf()];
    let mut read = 0;
    while let Some(path) = files.pop() {
        if path.is_dir() {
            files.extend(std::fs::read_dir(&path).unwrap().map(|e| e.unwrap().path()));
        } else {
            let bytes = std::fs::read(&path).unwrap();
            assert!(
                !String::from_utf8_lossy(&bytes).contains(&token),
                "{path:?}"
            );
            read += 1;
        }
    }
    assert!(read > 3, "the store holds the run");
}

#[test]
fn the_host_listens_on_loopback_only() {
    let running = Running::start("guard-loopback", |_| {});
    assert_eq!(running.address.ip(), IpAddr::from([127, 0, 0, 1]));
    let port = running.address.port();
    // Not on the other loopback, nor on this machine's address on a network, where it
    // has one: a connection there is refused.
    let v6 = SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], port));
    assert!(TcpStream::connect_timeout(&v6, Duration::from_secs(2)).is_err());
    let outward = UdpSocket::bind("0.0.0.0:0")
        .and_then(|socket| socket.connect("192.0.2.1:9").map(|()| socket))
        .and_then(|socket| socket.local_addr())
        .map(|address| address.ip());
    if let Ok(ip) = outward
        && !ip.is_loopback()
        && !ip.is_unspecified()
    {
        let there = SocketAddr::new(ip, port);
        assert!(TcpStream::connect_timeout(&there, Duration::from_secs(2)).is_err());
    }
    assert_eq!(running.api("GET", "/api/hello", None).status, 200);
}

#[test]
fn every_answer_carries_the_guards_headers_and_the_page_its_policy() {
    let running = Running::start("guard-headers", |_| {});
    let host = running.host_header();
    let answers = [
        running.api("GET", "/api/hello", None),
        running.api("GET", "/api/nothing", None),
        hello_with(&running, &[("Host", &host)]),
        hello_with(&running, &[("Host", "a.example")]),
        send(
            running.address,
            &request("GET", "/", &[("Host", &host)], None),
        ),
        send(running.address, b"NOT A REQUEST\r\n\r\n"),
    ];
    for answer in &answers {
        guarded_headers(answer);
    }
    let page = &answers[4];
    assert_eq!(page.status, 200);
    let policy = page.header("content-security-policy").unwrap();
    assert!(
        policy.starts_with("default-src 'self'; script-src 'self'"),
        "{policy}"
    );
    assert!(policy.contains("frame-ancestors 'none'"));
    // The page loads nothing from anywhere.
    assert!(!page.text().contains("http"), "{}", page.text());
    // The hello says the version and the store.
    let hello = answers[0].json();
    assert_eq!(hello["version"], env!("CARGO_PKG_VERSION"));
    assert!(hello["store"].as_str().unwrap().ends_with("store"));
}
