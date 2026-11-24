//! A front of the test's own, for the answers the pack's front never gives: a listener
//! on loopback (the test kit's) over TLS with a certificate of the test PKI, that reads
//! each request whole and answers what the test's script says of it, byte for byte.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};

use whizbang_backends::net::testing::Pki;
use whizbang_backends::pack::proto::{self, Message, Open, Role};
use whizbang_testkit::loopback::Listener;

/// One request the scripted front read.
#[derive(Debug, Clone, Default)]
pub struct Seen {
    pub line: String,
    pub head: String,
    pub body: Vec<u8>,
    /// The id of whizbang's hello, the first frame of the body.
    pub conversation: String,
}

impl Seen {
    /// The one value of a field, compared without regard to case.
    pub fn field(&self, name: &str) -> Option<String> {
        self.head.lines().find_map(|line| {
            let (field, value) = line.split_once(':')?;
            field
                .eq_ignore_ascii_case(name)
                .then(|| value.trim().to_owned())
        })
    }
}

/// What the scripted front does with one connection.
pub enum Step {
    /// Reads the request, writes what the closure makes of it, and closes.
    Answer(Piece),
    /// The same, then holds the connection open until the client closes it.
    AnswerAndHold(Piece),
    /// Reads the request, then writes each piece after its pause, then holds the
    /// connection open until the client closes it: a host that is slow between frames.
    Paced(Vec<(std::time::Duration, Piece)>),
    /// Reads the request and answers nothing, until the client closes it.
    Mute,
    /// Accepts the connection and says nothing at all, not even TLS's handshake.
    Silent,
    /// Accepts the connection and closes it at once, before TLS's handshake: a host
    /// whose door shuts on whoever knocks.
    Closed,
    /// Reads the request's head and closes the connection, answering nothing: a request
    /// cut after its first byte.
    CutAfterHead,
}

/// A piece of a paced answer, made of the request.
pub type Piece = Box<dyn Fn(&Seen) -> Vec<u8> + Send>;

/// The scripted front: its listener, and every request it read.
pub struct Scripted {
    pub listener: Listener,
    pub seen: Arc<Mutex<Vec<Seen>>>,
}

impl Scripted {
    pub fn base(&self) -> String {
        format!("https://127.0.0.1:{}", self.listener.port())
    }

    pub fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }
}

/// A scripted front of these steps, one connection each, in order.
pub fn scripted(pki: &Arc<Pki>, steps: Vec<Step>) -> Scripted {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let kept = seen.clone();
    let pki = pki.clone();
    let count = steps.len();
    let mut steps: Vec<Option<Step>> = steps.into_iter().map(Some).collect();
    let listener = Listener::serve(count, move |at, stream| {
        let step = steps[at].take().unwrap();
        serve(&pki, stream, step, &kept);
    });
    Scripted { listener, seen }
}

fn serve(pki: &Pki, stream: TcpStream, step: Step, seen: &Mutex<Vec<Seen>>) {
    if let Step::Closed = step {
        drop(stream);
        return;
    }
    if let Step::Silent = step {
        let mut stream = stream;
        let mut byte = [0u8; 1];
        while matches!(stream.read(&mut byte), Ok(read) if read > 0) {}
        return;
    }
    let mut tls = pki.accept(stream);
    if let Step::CutAfterHead = step {
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            match tls.read(&mut byte) {
                Ok(1) => head.push(byte[0]),
                _ => return,
            }
        }
        seen.lock().unwrap().push(Seen {
            head: String::from_utf8_lossy(&head).into_owned(),
            ..Seen::default()
        });
        return;
    }
    let Some(request) = read_request(&mut tls) else {
        return;
    };
    seen.lock().unwrap().push(request.clone());
    let hold = match step {
        Step::Answer(answer) => {
            let _ = tls.write_all(&answer(&request));
            let _ = tls.flush();
            false
        }
        Step::AnswerAndHold(answer) => {
            let _ = tls.write_all(&answer(&request));
            let _ = tls.flush();
            true
        }
        Step::Paced(pieces) => {
            for (pause, piece) in pieces {
                std::thread::sleep(pause);
                if tls
                    .write_all(&piece(&request))
                    .and_then(|()| tls.flush())
                    .is_err()
                {
                    return;
                }
            }
            true
        }
        Step::Mute => true,
        Step::Silent | Step::Closed | Step::CutAfterHead => false,
    };
    if hold {
        let mut byte = [0u8; 1];
        while matches!(tls.read(&mut byte), Ok(read) if read > 0) {}
    }
}

/// A request's line, head and body of its `Content-Length`.
fn read_request(from: &mut impl Read) -> Option<Seen> {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if from.read(&mut byte).ok()? == 0 {
            return None;
        }
        head.push(byte[0]);
    }
    let text = String::from_utf8_lossy(&head).into_owned();
    let (line, rest) = text.split_once("\r\n")?;
    let mut seen = Seen {
        line: line.to_owned(),
        head: rest.to_owned(),
        ..Seen::default()
    };
    let length: usize = seen
        .field("content-length")
        .and_then(|length| length.parse().ok())
        .unwrap_or(0);
    let mut body = vec![0; length];
    from.read_exact(&mut body).ok()?;
    if let Ok(Some(Message::Hello { id, .. })) = proto::read(&mut body.as_slice(), &mut Open::new())
    {
        seen.conversation = id;
    }
    seen.body = body;
    Some(seen)
}

// --- what an answer is made of -----------------------------------------------------------------

/// The worker's hello under whizbang's conversation id.
pub fn hello(seen: &Seen) -> Vec<u8> {
    let mut bytes = Vec::new();
    let hello = Message::Hello {
        id: seen.conversation.clone(),
        role: Role::Worker {
            device: Some("cpu".to_owned()),
            threads: 3,
        },
    };
    proto::write(&hello, &mut bytes).unwrap();
    bytes
}

/// These messages as frames.
pub fn frames(messages: &[Message]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for message in messages {
        proto::write(message, &mut bytes).unwrap();
    }
    bytes
}

/// An outcome record of this document and these lines, its lengths as given.
pub fn record(document: &serde_json::Value, lines: &[Vec<u8>]) -> Vec<u8> {
    let document = serde_json::to_vec(document).unwrap();
    let mut bytes = vec![0, 0, 0, 0];
    bytes.extend((document.len() as u32).to_be_bytes());
    bytes.extend(document);
    for line in lines {
        bytes.extend((line.len() as u32).to_be_bytes());
        bytes.extend(line);
    }
    bytes
}

/// The document of an outcome record.
pub fn outcome(
    outcome: &str,
    kind: Option<&str>,
    stage: &str,
    ended: serde_json::Value,
    lines: usize,
) -> serde_json::Value {
    serde_json::json!({
        "format": "pack-host/1",
        "outcome": outcome,
        "kind": kind,
        "admission": null,
        "stage": stage,
        "ended": ended,
        "lines": lines,
    })
}

/// A body in chunks, as the front writes one: each piece a chunk, then the last.
pub fn chunked(pieces: &[&[u8]]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for piece in pieces.iter().filter(|piece| !piece.is_empty()) {
        bytes.extend(format!("{:x}\r\n", piece.len()).into_bytes());
        bytes.extend(*piece);
        bytes.extend(b"\r\n");
    }
    bytes.extend(b"0\r\n\r\n");
    bytes
}

/// An answer's head, as the front sends it at once: chunked, naming nothing it learns
/// later.
pub fn head() -> Vec<u8> {
    b"HTTP/1.1 200 OK\r\nConnection: close\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec()
}

/// A keep-alive: the mark and a length of 0.
pub fn keep_alive() -> Vec<u8> {
    vec![0; 8]
}

/// The identity record naming this digest.
pub fn identity(digest: &str) -> Vec<u8> {
    record(
        &serde_json::json!({"format": "pack-host/1", "files": digest}),
        &[],
    )
}

/// A verified answer's head and its first chunks, a keep-alive and the identity record
/// naming this digest, before its other chunks.
pub fn verified_head(digest: &str) -> Vec<u8> {
    let mut bytes = head();
    bytes.extend(chunk(&keep_alive()));
    bytes.extend(chunk(&identity(digest)));
    bytes
}

/// One chunk of these bytes.
pub fn chunk(bytes: &[u8]) -> Vec<u8> {
    let mut chunk = format!("{:x}\r\n", bytes.len()).into_bytes();
    chunk.extend(bytes);
    chunk.extend(b"\r\n");
    chunk
}

/// A verified answer of these bytes of body.
pub fn verified(digest: &str, body: &[u8]) -> Vec<u8> {
    let mut bytes = verified_head(digest);
    bytes.extend(chunked(&[body]));
    bytes
}

/// An answer of a failure before the files were verified: a keep-alive, then its
/// record alone.
pub fn unverified(record: &[u8]) -> Vec<u8> {
    let mut bytes = head();
    bytes.extend(chunked(&[&keep_alive(), record]));
    bytes
}
