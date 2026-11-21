//! The fixture host of the playground's Chrome cases (plan m5, task U1, item 5): the
//! playground as it ships, started by Playwright's global setup in a new store, with fake
//! models in place of real ones. It is an example target, so the binary never holds it.
//!
//! It prints one line first, the page's address with the launch token, as `whizbang play
//! --no-open --stdin-quit --port 0` does (H1), so the test that started it can open the page:
//!
//! ```text
//! http://127.0.0.1:<port>/#k=<64 hex>
//! ```
//!
//! **Wave 1 of milestone 5: a stand-in.** The host is task H1's (`whizbang play`, in
//! `whizbang-serve`), built beside this task; until it lands, this file serves the
//! interface the binary embeds (`whizbang_serve::play::assets`) behind a stand-in of the
//! host's guard, with the plan's checks in the plan's order ("The playground API", "Who
//! may call"), and one route of the API, `GET /api/hello`, besides `POST /api/quit` and
//! the size and pixel checks of an upload. When H1 lands, whichever of the two lands
//! second replaces the stand-in by H1's host, with the test kit's fake segmenter and the
//! scripted fill and judge (chosen by `x-fixture-script`) as its models, and the Chrome
//! cases run unchanged against it.
//!
//! Options: `--store <folder>` (made if missing), `--port <n>` (0, the default, takes a
//! free port), `--stdin-quit` (quit when the standard input ends, so a test that dies
//! leaves no host behind).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use whizbang_core::identity::{DecodeLimits, IdentityError, identify};
use whizbang_serve::play::assets;
use whizbang_testkit::threads;

/// The longest the request line and headers may be.
const HEAD_LIMIT: usize = 16 * 1024;
/// The most a picture may hold.
const PICTURE_LIMIT: u64 = 64 * 1024 * 1024;
/// How long a connection may take to send its head, and to take its answer.
const DEADLINE: Duration = Duration::from_secs(5);
/// Connection workers. The acceptor only accepts and hands over.
const WORKERS: usize = 6;

/// The page's content security policy (plan m5, "Who may call").
const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' blob: data:; frame-src 'self'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

struct Host {
    port: u16,
    token: String,
    store: PathBuf,
}

struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

struct Answer {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
    page: bool,
    /// The host stops once this answer is written.
    quit: bool,
}

impl Answer {
    fn json(status: u16, value: serde_json::Value) -> Answer {
        Answer {
            status,
            content_type: "application/json",
            body: value.to_string().into_bytes(),
            page: false,
            quit: false,
        }
    }
    fn refused(status: u16, sentence: &str) -> Answer {
        Answer::json(status, serde_json::json!({ "error": sentence }))
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Content Too Large",
        415 => "Unsupported Media Type",
        421 => "Misdirected Request",
        431 => "Request Header Fields Too Large",
        501 => "Not Implemented",
        _ => "Error",
    }
}

fn main() {
    let mut store = std::env::temp_dir().join("whizbang-play-fixture");
    let mut port: u16 = 0;
    let mut stdin_quit = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--store" => store = args.next().map(PathBuf::from).unwrap_or(store),
            "--port" => port = args.next().and_then(|p| p.parse().ok()).unwrap_or(0),
            "--stdin-quit" => stdin_quit = true,
            other => fail(&format!("play_fixture: unknown argument {other}")),
        }
    }
    if let Err(error) = std::fs::create_dir_all(&store) {
        fail(&format!(
            "play_fixture: cannot make the store's folder: {error}"
        ));
    }
    let listener = TcpListener::bind(("127.0.0.1", port))
        .unwrap_or_else(|error| fail(&format!("play_fixture: cannot listen: {error}")));
    let port = listener
        .local_addr()
        .unwrap_or_else(|error| fail(&format!("play_fixture: no address: {error}")))
        .port();
    let host = Arc::new(Host {
        port,
        token: token(),
        store,
    });
    // The first line `whizbang play --no-open` prints when a test asks for its token.
    println!("http://127.0.0.1:{port}/#k={}", host.token);
    let _ = std::io::stdout().flush();

    if stdin_quit {
        threads::spawn(|| {
            let mut sink = Vec::new();
            let _ = std::io::stdin().read_to_end(&mut sink);
            std::process::exit(0);
        });
    }
    let (send, receive) = mpsc::channel::<TcpStream>();
    let receive = Arc::new(Mutex::new(receive));
    for _ in 0..WORKERS {
        let receive = Arc::clone(&receive);
        let host = Arc::clone(&host);
        threads::spawn(move || {
            loop {
                let next = match receive.lock() {
                    Ok(guard) => guard.recv(),
                    Err(_) => return,
                };
                match next {
                    Ok(stream) => serve(&host, stream),
                    Err(_) => return,
                }
            }
        });
    }
    for stream in listener.incoming().flatten() {
        if send.send(stream).is_err() {
            break;
        }
    }
}

fn fail(message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(2)
}

/// 32 bytes of the system's randomness, as 64 hexadecimal characters.
fn token() -> String {
    let mut bytes = [0u8; 32];
    let read = std::fs::File::open("/dev/urandom").and_then(|mut file| file.read_exact(&mut bytes));
    if let Err(error) = read {
        fail(&format!("play_fixture: no randomness: {error}"));
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Equal in time that does not depend on where the two differ.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn serve(host: &Host, stream: TcpStream) {
    let _ = stream.set_read_timeout(Some(DEADLINE));
    let _ = stream.set_write_timeout(Some(DEADLINE));
    let mut reader = BufReader::new(match stream.try_clone() {
        Ok(read) => read,
        Err(_) => return,
    });
    let answer = match read_head(&mut reader) {
        Ok(request) => answer(host, &request, &mut reader),
        Err(status) => Answer::refused(status, "The request's head is too large or malformed."),
    };
    write(stream, &answer);
    if answer.quit {
        std::process::exit(0);
    }
}

fn read_head(reader: &mut BufReader<TcpStream>) -> Result<Request, u16> {
    let mut lines = Vec::new();
    let mut size = 0;
    loop {
        let mut line = String::new();
        let read = reader
            .by_ref()
            .take((HEAD_LIMIT + 1 - size) as u64)
            .read_line(&mut line)
            .map_err(|_| 400u16)?;
        if read == 0 {
            return Err(400);
        }
        size += read;
        if size > HEAD_LIMIT {
            return Err(431);
        }
        let line = line.trim_end_matches(['\r', '\n']).to_string();
        if line.is_empty() {
            break;
        }
        lines.push(line);
    }
    let mut first = lines.first().ok_or(400u16)?.split(' ');
    let method = first.next().ok_or(400u16)?.to_string();
    let path = first.next().ok_or(400u16)?.to_string();
    let headers = lines[1..]
        .iter()
        .map(|line| {
            let (name, value) = line.split_once(':').ok_or(400u16)?;
            Ok((name.trim().to_string(), value.trim().to_string()))
        })
        .collect::<Result<Vec<_>, u16>>()?;
    Ok(Request {
        method,
        path,
        headers,
    })
}

/// The guard, in the plan's order, then the route.
fn answer(host: &Host, request: &Request, reader: &mut BufReader<TcpStream>) -> Answer {
    let port = host.port;
    let host_ok = request
        .header("host")
        .is_some_and(|h| h == format!("127.0.0.1:{port}") || h == format!("localhost:{port}"));
    if !host_ok {
        return Answer::refused(
            421,
            "This host answers only to 127.0.0.1 and localhost at its port.",
        );
    }
    let path = request.path.split(['?', '#']).next().unwrap_or("");
    let api = path.starts_with("/api/");
    if api {
        let given = request
            .header("authorization")
            .and_then(|value| value.strip_prefix("Bearer "))
            .unwrap_or("");
        if !same(given.as_bytes(), host.token.as_bytes()) {
            return Answer::refused(401, "unauthorized");
        }
        let origin = request.header("origin");
        let origin_ok = origin.is_none_or(|o| {
            o == format!("http://127.0.0.1:{port}") || o == format!("http://localhost:{port}")
        });
        if !origin_ok || (request.method != "GET" && origin.is_none()) {
            return Answer::refused(403, "A request from another origin is refused.");
        }
    }
    // A top-level navigation to a page (a GET outside /api/ that Chrome marks as one) may
    // come from anywhere, as `whizbang play`'s opening page does (H1): it reaches the static
    // files only, which are the binary's own public code, and the page acts on nothing in
    // its address but `#k=`.
    let navigation = !api
        && request.method == "GET"
        && request.header("sec-fetch-mode") == Some("navigate")
        && request.header("sec-fetch-dest") == Some("document");
    if let Some(site) = request.header("sec-fetch-site")
        && site != "same-origin"
        && site != "none"
        && !navigation
    {
        return Answer::refused(403, "A request from another site is refused.");
    }
    match (request.method.as_str(), path) {
        ("GET", "/api/hello") => Answer::json(
            200,
            serde_json::json!({
                "version": env!("CARGO_PKG_VERSION"),
                "store": host.store.display().to_string(),
            }),
        ),
        ("POST", "/api/quit") => Answer {
            quit: true,
            ..Answer::json(200, serde_json::json!({ "stopping": true }))
        },
        ("POST", "/api/pictures") => picture(request, reader),
        (_, path) if path.starts_with("/api/") => Answer::refused(404, "No such route."),
        ("GET", path) => {
            let file = path.strip_prefix('/').unwrap_or(path);
            let file = if file.is_empty() { assets::INDEX } else { file };
            match assets::find(file) {
                Some((content_type, bytes)) => Answer {
                    status: 200,
                    content_type,
                    body: bytes.to_vec(),
                    page: file == assets::INDEX,
                    quit: false,
                },
                None => Answer::refused(404, "No such file."),
            }
        }
        _ => Answer::refused(405, "Not a method this host takes."),
    }
}

/// An upload's size, type and pixels are checked as the host checks them; importing it is
/// task W1's, so a picture inside the limits is answered 501 here.
fn picture(request: &Request, reader: &mut BufReader<TcpStream>) -> Answer {
    let declared = request.header("content-type").unwrap_or("");
    if !declared.starts_with("image/") {
        return Answer::refused(415, "A picture is sent as an image.");
    }
    let Some(length) = request
        .header("content-length")
        .and_then(|value| value.parse::<u64>().ok())
    else {
        return Answer::refused(400, "A picture is sent with its length.");
    };
    if length > PICTURE_LIMIT {
        return Answer::refused(413, "A picture is at most 64 MiB.");
    }
    let mut body = Vec::new();
    if reader.take(length).read_to_end(&mut body).is_err() || body.len() as u64 != length {
        return Answer::refused(400, "The picture ended early.");
    }
    match identify(&body, &DecodeLimits::default()) {
        Err(IdentityError::Limit(why)) => {
            Answer::refused(413, &format!("The picture is too large: {why}."))
        }
        Err(error) => Answer::refused(
            415,
            &format!("That is not a picture the host can read: {error}."),
        ),
        Ok(_) => Answer::refused(501, "Importing a picture arrives with task W1."),
    }
}

fn write(mut stream: TcpStream, answer: &Answer) {
    let mut head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nCross-Origin-Resource-Policy: same-origin\r\n",
        answer.status,
        reason(answer.status),
        answer.content_type,
        answer.body.len()
    );
    if answer.page {
        head.push_str(&format!("Content-Security-Policy: {CSP}\r\n"));
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&answer.body);
    let _ = stream.flush();
    let _ = stream.shutdown(std::net::Shutdown::Write);
    // Read what the client may still send, briefly, so closing does not reset the answer.
    let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
    let mut sink = [0u8; 4096];
    while matches!(stream.read(&mut sink), Ok(n) if n > 0) {}
}
