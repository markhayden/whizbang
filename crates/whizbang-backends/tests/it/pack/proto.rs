//! The pack protocol's codec (ADR 0020): frames written and read strictly, every bound
//! looked at before the bytes it bounds, and the fixed vectors of
//! `pack/conformance/vectors/`, which the pack's own codec reads too.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Cursor;
use std::sync::Arc;

use serde_json::{Value, json};
use whizbang_backends::models::{InstallationId, ModelName};
use whizbang_backends::pack::proto::{
    self, Answer, Ask, Awaited, DIAGNOSTICS_LIMIT, EVENT_LIMIT, Found, HEADER_LIMIT,
    INSTANCE_HEADER_BYTES, Message, Open, RESULT_PAYLOAD_LIMIT, Raster, Reason, Refused, Role,
    SETTING_LIMIT, Scalar, Unwritten, VERSION,
};
use whizbang_core::capability::{
    Bounds, Declaration, Fault, FaultKind, INSTANCE_LIMIT, Picture, QUERY_LIMIT, Query,
    SegmentHandle, SegmentRequest, SegmentResponse, Segmenter, feature,
};
use whizbang_core::contract::{Cancellation, ModelIdentity, Secrets};
use whizbang_core::identity::{
    Binding, BindingId, CanonicalPixels, Digest, Direction, Mapping, PixelDigest, Space,
};

const WIDTH: u32 = 8;
const HEIGHT: u32 = 6;

// --- what the tests are made of ---------------------------------------------------------------

fn binding(name: &str) -> BindingId {
    BindingId(Digest::of_bytes(name.as_bytes()))
}

fn model() -> ModelName {
    ModelName::new("fake-ok", &Secrets::none()).unwrap()
}

fn installation() -> InstallationId {
    let text = Digest::of_bytes(b"an installation").to_string();
    InstallationId::from_text(&text, &Secrets::none()).unwrap()
}

/// The pixels of a picture of `WIDTH` by `HEIGHT`, every pixel its own colour.
fn rgba() -> Vec<u8> {
    let mut rgba = Vec::new();
    for at in 0..WIDTH * HEIGHT {
        rgba.extend([(at * 5) as u8, (at * 3 + 1) as u8, (255 - at) as u8, 255]);
    }
    rgba
}

fn image(under: BindingId) -> Raster {
    Raster {
        binding: under,
        width: WIDTH,
        height: HEIGHT,
        samples: rgba(),
    }
}

/// A plane of the picture's size with this box of whole pixels on.
fn plane(under: BindingId, [left, top, right, bottom]: [u32; 4]) -> Raster {
    let mut samples = vec![0; (WIDTH * HEIGHT) as usize];
    for y in top..bottom {
        for x in left..right {
            samples[(y * WIDTH + x) as usize] = 255;
        }
    }
    Raster {
        binding: under,
        width: WIDTH,
        height: HEIGHT,
        samples,
    }
}

fn found(under: BindingId, query: u32, score: f64, bounds: [u32; 4]) -> Found {
    Found {
        query,
        score,
        iou: Some(0.75),
        bounds: bounds.map(f64::from),
        plane: plane(under, bounds),
    }
}

fn queries() -> Vec<Query> {
    vec![
        Query::phrase("a dog").unwrap(),
        Query::point(1.5, 2.0).unwrap(),
        Query::boxed(1.0, 2.0, 3.0, 4.5).unwrap(),
    ]
}

fn segment(id: &str, under: BindingId) -> Message {
    Message::Request {
        id: id.to_owned(),
        ask: Ask::Segment {
            model: model(),
            installation: installation(),
            image: image(under),
            queries: queries(),
            max_instances: 2,
        },
    }
}

fn result(id: &str, found: Vec<Found>) -> Message {
    Message::Result {
        id: id.to_owned(),
        device: "cpu".to_owned(),
        threads: 3,
        diagnostics: String::new(),
        answer: Answer::Segment(found),
    }
}

fn event(id: &str, text: &str) -> Message {
    Message::Event {
        id: id.to_owned(),
        text: text.to_owned(),
    }
}

/// What a reader has open after it sent [`segment`] under this id and this binding.
fn awaiting(id: &str, under: BindingId) -> Open {
    let mut open = Open::new();
    let awaited = Awaited::segment(WIDTH, HEIGHT, under, 3, 2);
    open.expect(id, awaited).unwrap();
    open
}

fn written(message: &Message) -> Vec<u8> {
    let mut out = Vec::new();
    proto::write(message, &mut out).unwrap();
    out
}

/// Why the writer refuses a message, having written nothing of it.
fn unwritten(message: &Message) -> Reason {
    let mut out = Vec::new();
    let refused = match proto::write(message, &mut out) {
        Err(Unwritten::Refused(refused)) => refused,
        other => panic!("not refused: {other:?}"),
    };
    assert!(out.is_empty(), "{} bytes were written", out.len());
    refused.reason()
}

fn read(bytes: &[u8], open: &mut Open) -> Result<Option<Message>, Refused> {
    proto::read(&mut Cursor::new(bytes), open)
}

/// The one message these bytes hold, and nothing after it.
fn one(bytes: &[u8], open: &mut Open) -> Message {
    let mut input = Cursor::new(bytes);
    let message = proto::read(&mut input, open).unwrap().unwrap();
    assert_eq!(input.position(), bytes.len() as u64, "bytes were left");
    assert_eq!(proto::read(&mut input, open), Ok(None));
    message
}

fn refused(bytes: &[u8], open: &mut Open) -> Reason {
    match read(bytes, open) {
        Err(refused) => refused.reason(),
        Ok(message) => panic!("not refused: {message:?}, of {} bytes", bytes.len()),
    }
}

/// A frame as ADR 0020 lays one out, made here by hand: four bytes of the header's
/// length, the header, and each payload after eight bytes of its length.
fn frame(header: &[u8], payloads: &[&[u8]]) -> Vec<u8> {
    let mut frame = (header.len() as u32).to_be_bytes().to_vec();
    frame.extend(header);
    for payload in payloads {
        frame.extend((payload.len() as u64).to_be_bytes());
        frame.extend(*payload);
    }
    frame
}

/// The header of a frame the codec wrote, as JSON, and the bytes after it.
fn taken_apart(frame: &[u8]) -> (Value, Vec<u8>) {
    let length = u32::from_be_bytes(frame[..4].try_into().unwrap()) as usize;
    let header = serde_json::from_slice(&frame[4..4 + length]).unwrap();
    (header, frame[4 + length..].to_vec())
}

/// The frame of `message` with its header changed. The changed header is written with
/// its fields in the order of their names, which is not the canonical one: every rule
/// these tests break with it is looked at before the canonical writing is.
fn changed(message: &Message, change: impl FnOnce(&mut Value)) -> Vec<u8> {
    let (mut header, rest) = taken_apart(&written(message));
    change(&mut header);
    let mut bytes = frame(&serde_json::to_vec(&header).unwrap(), &[]);
    bytes.extend(rest);
    bytes
}

/// A number with a fraction, as a header writes one.
fn bits(value: f64) -> String {
    format!("{:016x}", value.to_bits())
}

/// One of each message, by a name: every shape `pack/1` has.
fn every_message() -> Vec<(&'static str, Message, Open)> {
    let under = binding("a picture");
    let mut settings = BTreeMap::new();
    settings.insert("outline".to_owned(), Scalar::Text("whole".to_owned()));
    settings.insert("pad".to_owned(), Scalar::Whole(-5));
    settings.insert("strict".to_owned(), Scalar::Flag(true));
    let hello = |role| Message::Hello {
        id: "h1".to_owned(),
        role,
    };
    let ask = |ask| Message::Request {
        id: "r1".to_owned(),
        ask,
    };
    let done = |answer| Message::Result {
        id: "r1".to_owned(),
        device: "cpu".to_owned(),
        threads: 3,
        diagnostics: "loaded in 12 ms".to_owned(),
        answer,
    };
    let opened = |awaited| {
        let mut open = Open::new();
        open.expect("r1", awaited).unwrap();
        open
    };
    let worker = Role::Worker {
        device: Some("cpu".to_owned()),
        threads: 3,
    };
    let load = Ask::Load {
        model: model(),
        installation: installation(),
        settings,
    };
    let unload = Ask::Unload {
        model: model(),
        installation: installation(),
    };
    let instances = vec![
        found(under, 0, 0.9, [2, 1, 5, 4]),
        found(under, 2, 0.5, [0, 0, 2, 2]),
    ];
    vec![
        ("hello-whizbang", hello(Role::Whizbang), Open::new()),
        ("hello-worker", hello(worker), Open::new()),
        ("request-load", ask(load), Open::new()),
        ("request-unload", ask(unload), Open::new()),
        ("request-segment", segment("r1", under), Open::new()),
        ("result-load", done(Answer::Load), opened(Awaited::load())),
        (
            "result-unload",
            done(Answer::Unload),
            opened(Awaited::unload()),
        ),
        (
            "result-segment",
            result("r1", instances),
            awaiting("r1", under),
        ),
        ("event", event("r1", "loading"), awaiting("r1", under)),
        (
            "cancel",
            Message::Cancel {
                id: "r1".to_owned(),
            },
            Open::new(),
        ),
        (
            "error",
            Message::Error {
                id: "r1".to_owned(),
                fault: FaultKind::Unavailable,
                sentence: "the model is not loaded".to_owned(),
            },
            awaiting("r1", under),
        ),
    ]
}

// --- the fixed vectors --------------------------------------------------------------------------

/// The most vector files the folder is read for: the loop stops there, whatever the
/// folder holds.
const VECTORS_MOST: usize = 256;

fn hex(text: &str) -> Vec<u8> {
    assert!(text.len().is_multiple_of(2), "an odd count of hex digits");
    let digit = |byte: u8| (byte as char).to_digit(16).unwrap() as u8;
    text.as_bytes()
        .chunks(2)
        .map(|pair| digit(pair[0]) << 4 | digit(pair[1]))
        .collect()
}

/// Every vector of the folder, in the order of the files' names: its name and what
/// its file holds.
fn vectors() -> Vec<(String, Value)> {
    let mut names: Vec<String> = std::fs::read_dir(super::vectors())
        .unwrap()
        .take(VECTORS_MOST)
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".json"))
        .collect();
    names.sort();
    assert!(names.len() < VECTORS_MOST, "the folder was not read whole");
    names
        .into_iter()
        .map(|file| {
            let bytes = std::fs::read(super::vectors().join(&file)).unwrap();
            let vector: Value = serde_json::from_slice(&bytes).unwrap();
            let name = file.trim_end_matches(".json").to_owned();
            assert_eq!(
                vector["name"],
                json!(name),
                "a vector is named as its file is"
            );
            (name, vector)
        })
        .collect()
}

/// What a vector says its reader has open.
fn open_of(vector: &Value) -> Open {
    let mut open = Open::new();
    for request in vector["open"].as_array().unwrap() {
        let whole = |field: &str| request[field].as_u64().unwrap();
        let awaited = match request["kind"].as_str().unwrap() {
            "load" => Awaited::load(),
            "unload" => Awaited::unload(),
            "segment" => {
                let under: Digest = request["binding"].as_str().unwrap().parse().unwrap();
                Awaited::segment(
                    whole("width") as u32,
                    whole("height") as u32,
                    BindingId(under),
                    whole("queries") as u32,
                    whole("max_instances") as u32,
                )
            }
            other => panic!("{other} is no kind"),
        };
        let awaited = awaited.after(whole("events") as u32, whole("text"));
        open.expect(request["id"].as_str().unwrap(), awaited)
            .unwrap();
    }
    open
}

/// A payload as a vector states it: how large it is, what it belongs to, and the
/// digest of its samples.
fn stated(raster: &Raster) -> Value {
    json!({
        "width": raster.width,
        "height": raster.height,
        "binding": raster.binding.to_string(),
        "bytes": raster.samples.len(),
        "sha256": Digest::of_bytes(&raster.samples).hex(),
    })
}

/// A message as a vector states it: its fields in plain values, each number with a
/// fraction as a number, and each payload by its size and its digest.
fn described(message: &Message) -> Value {
    match message {
        Message::Hello { id, role } => match role {
            Role::Whizbang => {
                json!({"type": "hello", "id": id, "version": VERSION, "role": "whizbang"})
            }
            Role::Worker { device, threads } => json!({
                "type": "hello", "id": id, "version": VERSION, "role": "worker",
                "device": device, "threads": threads,
            }),
        },
        Message::Request { id, ask } => match ask {
            Ask::Load {
                model,
                installation,
                settings,
            } => {
                let settings: BTreeMap<&str, Value> = settings
                    .iter()
                    .map(|(name, value)| {
                        let value = match value {
                            Scalar::Text(text) => json!(text),
                            Scalar::Whole(whole) => json!(whole),
                            Scalar::Flag(flag) => json!(flag),
                        };
                        (name.as_str(), value)
                    })
                    .collect();
                json!({
                    "type": "request", "id": id, "kind": "load", "model": model.as_str(),
                    "installation": installation.to_string(), "settings": settings,
                })
            }
            Ask::Unload {
                model,
                installation,
            } => json!({
                "type": "request", "id": id, "kind": "unload", "model": model.as_str(),
                "installation": installation.to_string(),
            }),
            Ask::Segment {
                model,
                installation,
                image,
                queries,
                max_instances,
            } => {
                let queries: Vec<Value> = queries
                    .iter()
                    .map(|query| match query.asked() {
                        whizbang_core::capability::Asked::Phrase(phrase) => {
                            json!({"phrase": phrase})
                        }
                        whizbang_core::capability::Asked::Point { x, y } => {
                            json!({"point": [x, y]})
                        }
                        whizbang_core::capability::Asked::Box {
                            left,
                            top,
                            right,
                            bottom,
                        } => json!({"box": [left, top, right, bottom]}),
                    })
                    .collect();
                json!({
                    "type": "request", "id": id, "kind": "segment", "model": model.as_str(),
                    "installation": installation.to_string(), "image": stated(image),
                    "queries": queries, "max_instances": max_instances,
                })
            }
        },
        Message::Result {
            id,
            device,
            threads,
            diagnostics,
            answer,
        } => {
            let mut said = json!({
                "type": "result", "id": id, "device": device, "threads": threads,
                "diagnostics": diagnostics,
            });
            said["kind"] = match answer {
                Answer::Load => json!("load"),
                Answer::Unload => json!("unload"),
                Answer::Segment(found) => {
                    let instances: Vec<Value> = found
                        .iter()
                        .map(|found| {
                            json!({
                                "query": found.query, "score": found.score, "iou": found.iou,
                                "box": found.bounds, "plane": stated(&found.plane),
                            })
                        })
                        .collect();
                    said["instances"] = json!(instances);
                    json!("segment")
                }
            };
            said
        }
        Message::Event { id, text } => json!({"type": "event", "id": id, "text": text}),
        Message::Cancel { id } => json!({"type": "cancel", "id": id}),
        Message::Error {
            id,
            fault,
            sentence,
        } => json!({
            "type": "error", "id": id, "fault": fault.name(), "sentence": sentence,
        }),
    }
}

/// Which shape of message a description is: its type, and its kind or its role.
fn shape(described: &Value) -> String {
    let part = |field: &str| described[field].as_str().map(str::to_owned);
    let second = part("kind").or(part("role").filter(|_| described["type"] == "hello"));
    match second {
        Some(second) => format!("{} {second}", described["type"].as_str().unwrap()),
        None => part("type").unwrap(),
    }
}

#[test]
fn every_vector_parses_to_what_it_states_and_is_written_back_byte_for_byte() {
    let (mut shapes, mut reasons) = (BTreeSet::new(), BTreeSet::new());
    for (name, vector) in vectors() {
        let frame = hex(vector["frame"].as_str().unwrap());
        let mut open = open_of(&vector);
        assert!(vector["says"].as_str().is_some_and(|says| !says.is_empty()));
        match vector.get("refused") {
            None => {
                let message = one(&frame, &mut open);
                assert_eq!(described(&message), vector["message"], "{name}");
                assert_eq!(written(&message), frame, "{name}");
                assert!(!name.starts_with("refused-"), "{name}");
                shapes.insert(shape(&vector["message"]));
            }
            Some(reason) => {
                assert!(vector.get("message").is_none(), "{name}");
                let said = refused(&frame, &mut open);
                assert_eq!(json!(said.name()), *reason, "{name}");
                assert!(name.starts_with("refused-"), "{name}");
                reasons.insert(said);
            }
        }
    }
    // Each message is there as bytes, in every shape it has; and each refusal.
    let expected = [
        "cancel",
        "error",
        "event",
        "hello whizbang",
        "hello worker",
        "request load",
        "request segment",
        "request unload",
        "result load",
        "result segment",
        "result unload",
    ];
    assert_eq!(
        shapes.iter().map(String::as_str).collect::<Vec<_>>(),
        expected
    );
    let every: BTreeSet<Reason> = Reason::ALL.into_iter().collect();
    assert_eq!(reasons, every);
    assert_eq!(every.len(), 17);
    // A reason's name is one word of its own, and what a vector calls it.
    let names: BTreeSet<&str> = Reason::ALL.into_iter().map(Reason::name).collect();
    assert_eq!(names.len(), Reason::ALL.len());
}

#[test]
fn every_message_is_read_back_as_it_was_written() {
    for (name, message, mut open) in every_message() {
        let bytes = written(&message);
        assert_eq!(one(&bytes, &mut open), message, "{name}");
    }
}

#[test]
fn a_frame_cut_at_any_byte_is_an_error_and_never_a_shorter_message() {
    let mut cuts = 0;
    for (name, vector) in vectors() {
        if vector.get("refused").is_some() {
            continue;
        }
        let frame = hex(vector["frame"].as_str().unwrap());
        // No byte at all is the end of the conversation, between two frames.
        assert_eq!(read(&[], &mut open_of(&vector)), Ok(None), "{name}");
        for cut in 1..frame.len() {
            let refused = read(&frame[..cut], &mut open_of(&vector));
            let reason = refused.map_err(|refused| refused.reason());
            assert_eq!(reason, Err(Reason::Truncated), "{name} cut at {cut}");
            cuts += 1;
        }
        // And a frame cut short is not helped by the next frame's bytes: what follows
        // a whole frame is read as the next one.
        let mut two = frame.clone();
        two.extend(&frame[..frame.len() - 1]);
        let mut open = open_of(&vector);
        let mut input = Cursor::new(&two[..]);
        assert!(proto::read(&mut input, &mut open).unwrap().is_some());
        assert_eq!(input.position(), frame.len() as u64, "{name}");
    }
    assert!(cuts > 2_000, "{cuts} cuts were tried");
}

// --- the hello -----------------------------------------------------------------------------------

#[test]
fn a_hello_of_another_version_is_refused_and_says_both_versions() {
    let hello = Message::Hello {
        id: "h1".to_owned(),
        role: Role::Whizbang,
    };
    assert_eq!(VERSION, "pack/1");
    assert_eq!(one(&written(&hello), &mut Open::new()), hello);
    // Another version, and nothing else changed.
    let other = changed(&hello, |header| header["version"] = json!("pack/2"));
    let said = read(&other, &mut Open::new()).unwrap_err();
    assert_eq!(said.reason(), Reason::Version);
    assert_eq!(
        said.sentence(),
        "this side speaks pack/1 and the other said pack/2"
    );
    assert_eq!(said.to_string(), said.sentence());
    // Another version may have other fields, and lack these: the version is looked at
    // before any of them.
    let later = br#"{"type":"hello","version":"pack/2","features":["x"]}"#;
    let said = read(&frame(later, &[]), &mut Open::new()).unwrap_err();
    assert_eq!(said.reason(), Reason::Version);
    // No range and no list is a version: each is refused, and nothing is negotiated.
    for version in [
        "pack/1,pack/2",
        "pack/1-2",
        "pack/1.0",
        "PACK/1",
        "pack/01",
        "",
    ] {
        let other = changed(&hello, |header| header["version"] = json!(version));
        assert_eq!(
            refused(&other, &mut Open::new()),
            Reason::Version,
            "{version}"
        );
    }
    // What the other side said is quoted only when it is a plain name.
    for version in ["pack/2 or \"anything\"", "/etc/passwd\n", &"v".repeat(33)] {
        let other = changed(&hello, |header| header["version"] = json!(version));
        let said = read(&other, &mut Open::new()).unwrap_err();
        assert_eq!(
            said.sentence(),
            "this side speaks pack/1 and the other said a version that is not a name"
        );
    }
    // A hello that states none, or something that is no text.
    let none = changed(&hello, |header| {
        header.as_object_mut().unwrap().remove("version");
    });
    assert_eq!(refused(&none, &mut Open::new()), Reason::MissingField);
    let number = changed(&hello, |header| header["version"] = json!(1));
    assert_eq!(refused(&number, &mut Open::new()), Reason::BadValue);
    // After the version, the role, which says which fields the hello has: a role that
    // is neither name is refused for its value though a field that is no hello's is
    // there too, and one that is missing is a missing field.
    let roled = |role: Value| {
        changed(&hello, |header| {
            header["role"] = role;
            header["extra"] = json!(1);
        })
    };
    assert_eq!(
        refused(&roled(json!("server")), &mut Open::new()),
        Reason::BadValue
    );
    assert_eq!(
        refused(&roled(json!(1)), &mut Open::new()),
        Reason::BadValue
    );
    assert_eq!(
        refused(&roled(json!("whizbang")), &mut Open::new()),
        Reason::UnknownField
    );
    let no_role = changed(&hello, |header| {
        header.as_object_mut().unwrap().remove("role");
        header["extra"] = json!(1);
    });
    assert_eq!(refused(&no_role, &mut Open::new()), Reason::MissingField);
}

#[test]
fn a_header_is_strict_json_in_strict_utf8_and_each_edge_of_it_has_one_reason() {
    let reason = |header: &[u8]| refused(&frame(header, &[]), &mut Open::new());
    // What is not JSON, or not strictly, is refused before any field is looked at,
    // though a lenient reader would take each and refuse it later or not at all.
    let not_json: [&[u8]; 28] = [
        // A byte-order mark.
        b"\xef\xbb\xbf{\"type\":\"cancel\",\"id\":\"r1\"}",
        // Surrogates encoded as bytes, which strict UTF-8 refuses.
        b"{\"type\":\"cancel\",\"id\":\"r\xed\xa0\x80\"}",
        b"{\"type\":\"cancel\",\"id\":\"r\xed\xbf\xbf\"}",
        // Escaped surrogates that are not a pair, which name no character.
        b"{\"type\":\"cancel\",\"id\":\"r\\ud800\"}",
        b"{\"type\":\"cancel\",\"id\":\"r\\udc00\"}",
        b"{\"type\":\"cancel\",\"id\":\"r\\ud800\\u0041\"}",
        // A number outside a double's range.
        b"{\"type\":\"cancel\",\"id\":1e400}",
        b"{\"type\":\"cancel\",\"id\":-1e400}",
        // A field given twice.
        b"{\"type\":\"cancel\",\"id\":\"r1\",\"id\":\"r1\"}",
        // NaN and the infinities.
        b"{\"type\":\"cancel\",\"id\":NaN}",
        b"{\"type\":\"cancel\",\"id\":Infinity}",
        b"{\"type\":\"cancel\",\"id\":-Infinity}",
        // A control character written as itself in a text.
        b"{\"type\":\"cancel\",\"id\":\"r\n1\"}",
        b"{\"type\":\"cancel\",\"id\":\"r\t1\"}",
        // And what JSON's grammar has not: numbers it does not write, escapes it has
        // not, a comma with nothing after it, a key that is no text, a value cut short.
        b"{\"type\":\"cancel\",\"id\":01}",
        b"{\"type\":\"cancel\",\"id\":1.}",
        b"{\"type\":\"cancel\",\"id\":.5}",
        b"{\"type\":\"cancel\",\"id\":+1}",
        b"{\"type\":\"cancel\",\"id\":1e}",
        b"{\"type\":\"cancel\",\"id\":-}",
        b"{\"type\":\"cancel\",\"id\":\"r\\x1\"}",
        b"{\"type\":\"cancel\",\"id\":\"r\\u00g1\"}",
        b"{\"type\":\"cancel\",\"id\":\"r1\",}",
        b"{\"type\":\"cancel\",\"id\":[1,]}",
        b"{type:\"cancel\",\"id\":\"r1\"}",
        b"{\"type\":\"cancel\",\"id\":tru}",
        b"{\"type\":\"cancel\",\"id\":\"r1}",
        b" ",
    ];
    for header in not_json {
        assert_eq!(
            reason(header),
            Reason::NotJson,
            "{}",
            String::from_utf8_lossy(header)
        );
    }
    // JSON nested 128 deep, counting the header's own object as the first, is not
    // JSON here; 127 deep is, and is refused for its value.
    let nested = |depth: usize| {
        let (open, close) = ("[".repeat(depth - 1), "]".repeat(depth - 1));
        format!("{{\"type\":\"cancel\",\"id\":{open}{close}}}").into_bytes()
    };
    assert_eq!(reason(&nested(2)), Reason::BadValue);
    assert_eq!(reason(&nested(127)), Reason::BadValue);
    assert_eq!(reason(&nested(128)), Reason::NotJson);
    assert_eq!(reason(&nested(4_096)), Reason::NotJson);
    // What JSON has and the header's rules refuse is refused for its value: a whole
    // number past 64 bits, an escaped separator, a text where a list is asked.
    let cancel = |id: &str| reason(format!("{{\"type\":\"cancel\",\"id\":{id}}}").as_bytes());
    for id in [
        "18446744073709551616",
        "-9223372036854775809",
        "1e-400",
        "\"r\\/1\"",
        "{}",
    ] {
        assert_eq!(cancel(id), Reason::BadValue, "{id}");
    }
    // White space is JSON, and is refused only for the writing.
    assert_eq!(cancel(" \"r1\" "), Reason::NotCanonical);
    assert_eq!(cancel("\"r\\u0031\""), Reason::NotCanonical);

    // `-0` is not a whole number: where one is asked it is a bad value, and not the
    // canonical writing of 0. Each header below is canonical but for the `-`.
    let under = binding("a picture");
    let messages: BTreeMap<&str, (Message, Open)> = every_message()
        .into_iter()
        .map(|(name, message, open)| (name, (message, open)))
        .collect();
    let replaced = |name: &str, old: &str, new: &str| {
        let (message, open) = &messages[name];
        let bytes = written(message);
        let (_, rest) = taken_apart(&bytes);
        let length = bytes.len() - rest.len();
        let text = String::from_utf8(bytes[4..length].to_vec()).unwrap();
        assert!(text.contains(old), "{name} {old}");
        let mut changed = frame(text.replacen(old, new, 1).as_bytes(), &[]);
        changed.extend(rest);
        let mut again = Open::new();
        if let Some(awaited) = open.awaited("r1") {
            again.expect("r1", awaited.clone()).unwrap();
        }
        read(&changed, &mut again)
            .map(|_| ())
            .map_err(|said| said.reason())
    };
    let zeros = [
        ("hello-worker", "\"threads\":3", "\"threads\":-0"),
        ("result-load", "\"threads\":3", "\"threads\":-0"),
        ("request-load", "\"value\":-5", "\"value\":-0"),
        (
            "request-segment",
            "\"max_instances\":2",
            "\"max_instances\":-0",
        ),
        ("request-segment", "\"width\":8", "\"width\":-0"),
        ("result-segment", "\"query\":0", "\"query\":-0"),
        // Nor is a flag a whole number.
        ("hello-worker", "\"threads\":3", "\"threads\":true"),
        ("request-load", "\"value\":-5", "\"value\":-5.0"),
    ];
    for (name, old, new) in zeros {
        assert_eq!(
            replaced(name, old, new),
            Err(Reason::BadValue),
            "{name} {new}"
        );
    }
    // A setting's value may be 0, written as it is.
    assert_eq!(
        replaced("request-load", "\"value\":-5", "\"value\":0"),
        Ok(())
    );
    // An escaped pair of surrogates is JSON, and names one character, which the
    // canonical writing writes as itself.
    let pair = ("\"phrase\":\"a dog\"", "\"phrase\":\"a dog\\ud83d\\ude00\"");
    assert_eq!(
        replaced("request-segment", pair.0, pair.1),
        Err(Reason::NotCanonical)
    );
    let itself = "\"phrase\":\"a dog\u{1f600}\"";
    assert_eq!(replaced("request-segment", pair.0, itself), Ok(()));

    // A number with a fraction may be -0 wherever its field takes 0, and is read and
    // written back with its own bits: a score, a predicted IoU, an edge of a box, a
    // coordinate of a point.
    let negative = Found {
        query: 0,
        score: -0.0,
        iou: Some(-0.0),
        bounds: [-0.0, -0.0, 2.0, 2.0],
        plane: plane(under, [0, 0, 2, 2]),
    };
    let answer = result("r1", vec![negative]);
    let bytes = written(&answer);
    let (header, _) = taken_apart(&bytes);
    let zero = json!("8000000000000000");
    assert_eq!(header["instances"][0]["score"], zero);
    assert_eq!(header["instances"][0]["iou"], zero);
    assert_eq!(header["instances"][0]["box"][1], zero);
    let read_back = one(&bytes, &mut awaiting("r1", under));
    assert_eq!(written(&read_back), bytes);
    let Message::Result {
        answer: Answer::Segment(found),
        ..
    } = &read_back
    else {
        panic!("not a segment's result");
    };
    assert!(found[0].score.is_sign_negative());
    assert!(found[0].iou.unwrap().is_sign_negative());
    assert!(found[0].bounds[0].is_sign_negative());
    let asked = Message::Request {
        id: "r1".to_owned(),
        ask: Ask::Segment {
            model: model(),
            installation: installation(),
            image: image(under),
            queries: vec![
                Query::point(-0.0, -0.0).unwrap(),
                Query::boxed(-0.0, -0.0, 1.0, 1.0).unwrap(),
            ],
            max_instances: 1,
        },
    };
    let bytes = written(&asked);
    let (header, _) = taken_apart(&bytes);
    assert_eq!(header["queries"][0]["point"][1], zero);
    assert_eq!(header["queries"][1]["box"][0], zero);
    assert_eq!(written(&one(&bytes, &mut Open::new())), bytes);
}

// --- bounds -----------------------------------------------------------------------------------

/// An event whose header is exactly this many bytes.
fn event_of(header: u64) -> Message {
    let least = written(&event("r1", "x")).len() as u64 - 4 - 1;
    event("r1", &"x".repeat((header - least) as usize))
}

#[test]
fn a_header_or_a_payload_over_its_bound_is_refused_before_it_is_read() {
    let under = binding("a picture");
    assert_eq!(HEADER_LIMIT, 65_536);

    // A header of exactly the bound is read; one byte more is refused.
    let at = written(&event_of(HEADER_LIMIT));
    assert_eq!(at.len() as u64, 4 + HEADER_LIMIT);
    assert!(read(&at, &mut awaiting("r1", under)).unwrap().is_some());
    let over = frame(&vec![b' '; HEADER_LIMIT as usize + 1], &[]);
    assert_eq!(refused(&over, &mut Open::new()), Reason::HeaderTooLarge);
    // It is refused from its four bytes alone, before the header is read: with nothing
    // after them the answer is the same, where a header within the bound that is not
    // there is a cut frame.
    for length in [HEADER_LIMIT as u32 + 1, u32::MAX, 0] {
        let stated = length.to_be_bytes();
        assert_eq!(
            refused(&stated, &mut Open::new()),
            Reason::HeaderTooLarge,
            "{length}"
        );
    }
    for length in [HEADER_LIMIT as u32, 1] {
        let stated = length.to_be_bytes();
        assert_eq!(refused(&stated, &mut Open::new()), Reason::Truncated);
    }
    // The writer holds a header to the same bound, and writes nothing of one over it.
    assert_eq!(
        unwritten(&event_of(HEADER_LIMIT + 1)),
        Reason::HeaderTooLarge
    );

    // A payload. Its length is stated twice, by its entry and in the eight bytes before
    // it; one that is over what its entry gives is refused before a sample is read.
    let request = segment("r1", under);
    let whole = written(&request);
    let length = 4 + u32::from_be_bytes(whole[..4].try_into().unwrap()) as usize;
    let header = whole[4..length].to_vec();
    let expected = (4 * WIDTH * HEIGHT) as u64;
    let stating = |length: u64| {
        let mut bytes = frame(&header, &[]);
        bytes.extend(length.to_be_bytes());
        bytes
    };
    for length in [expected + 1, expected - 1, u64::MAX, 0] {
        assert_eq!(
            refused(&stating(length), &mut Open::new()),
            Reason::WrongLength,
            "{length}"
        );
    }
    assert_eq!(
        refused(&stating(expected), &mut Open::new()),
        Reason::Truncated
    );

    // The picture itself: 50,000,000 pixels at most, known from the header. At the
    // bound the header is taken, and the reader goes on to a payload that is not
    // there; one row more is refused though nothing follows the header.
    let sized = |width: u32, height: u32| {
        changed(&request, |header| {
            header["image"]["width"] = json!(width);
            header["image"]["height"] = json!(height);
        })
    };
    let canonical = |width: u32, height: u32| {
        let text = String::from_utf8(header.clone()).unwrap();
        let old = format!("\"width\":{WIDTH},\"height\":{HEIGHT}");
        assert!(text.contains(&old));
        let new = format!("\"width\":{width},\"height\":{height}");
        frame(text.replace(&old, &new).as_bytes(), &[])
    };
    assert_eq!(
        refused(&canonical(10_000, 5_000), &mut Open::new()),
        Reason::Truncated
    );
    assert_eq!(
        refused(&canonical(10_000, 5_001), &mut Open::new()),
        Reason::OverBound
    );
    assert_eq!(
        refused(&sized(50_000_001, 1), &mut Open::new()),
        Reason::OverBound
    );
    assert_eq!(refused(&sized(0, 1), &mut Open::new()), Reason::BadValue);

    // A result: no more instances for a query than its request asked, and each plane
    // the size of the request's image. Both are known from the header, and refused
    // before a plane is read: nothing follows these headers.
    let answer = |found: Vec<Found>| {
        let whole = written(&result("r1", found));
        let length = u32::from_be_bytes(whole[..4].try_into().unwrap()) as usize;
        whole[..4 + length].to_vec()
    };
    let two = vec![
        found(under, 0, 0.9, [2, 1, 5, 4]),
        found(under, 0, 0.8, [0, 0, 2, 2]),
    ];
    let mut three = two.clone();
    three.push(found(under, 0, 0.7, [0, 0, 1, 1]));
    assert_eq!(
        refused(&answer(two), &mut awaiting("r1", under)),
        Reason::Truncated
    );
    assert_eq!(
        refused(&answer(three), &mut awaiting("r1", under)),
        Reason::OverBound
    );
    let mut wide = found(under, 0, 0.9, [2, 1, 5, 4]);
    wide.plane.width += 1;
    wide.plane.samples = vec![255; ((WIDTH + 1) * HEIGHT) as usize];
    assert_eq!(
        refused(&answer(vec![wide]), &mut awaiting("r1", under)),
        Reason::WrongSize
    );
    // An instance for a query the request did not hold.
    let beyond = vec![found(under, 3, 0.9, [2, 1, 5, 4])];
    assert_eq!(
        refused(&answer(beyond), &mut awaiting("r1", under)),
        Reason::BadValue
    );
}

/// The header of a segment result with these instances' entries, written here by
/// hand in the canonical writing.
fn result_header(id: &str, diagnostics: &str, entries: &[String]) -> Vec<u8> {
    format!(
        "{{\"type\":\"result\",\"id\":\"{id}\",\"kind\":\"segment\",\"device\":\"cpu\",\
         \"threads\":3,\"diagnostics\":\"{diagnostics}\",\"instances\":[{}]}}",
        entries.join(",")
    )
    .into_bytes()
}

/// One instance's entry in a result's header, in the canonical writing.
fn entry(found: &Found) -> String {
    let (width, height, under) = (found.plane.width, found.plane.height, found.plane.binding);
    let (query, bounds) = (found.query, found.bounds);
    let iou = match found.iou {
        Some(iou) => format!("\"{}\"", bits(iou)),
        None => "null".to_owned(),
    };
    format!(
        "{{\"query\":{query},\"score\":\"{}\",\"iou\":{iou},\"box\":[\"{}\",\"{}\",\"{}\",\"{}\"],\
         \"plane\":{{\"width\":{width},\"height\":{height},\"layout\":\"plane\",\"data\":\"u8\",\
         \"colour\":\"none\",\"alpha\":\"none\",\"binding\":\"{under}\"}}}}",
        bits(found.score),
        bits(bounds[0]),
        bits(bounds[1]),
        bits(bounds[2]),
        bits(bounds[3]),
    )
}

#[test]
fn a_results_header_is_bounded_from_its_request_and_holds_the_most_instances_adr_0006_allows() {
    let under = binding("a picture");
    assert_eq!(INSTANCE_HEADER_BYTES, 512);
    let most = QUERY_LIMIT as u32 * INSTANCE_LIMIT;
    assert_eq!(most, 4_096);
    let bound = |queries: u32, instances: u32| {
        Awaited::segment(1, 1, under, queries, instances).header_bound()
    };
    assert_eq!(bound(1, 1), 65_536 + 512);
    assert_eq!(bound(3, 2), 65_536 + 6 * 512);
    assert_eq!(bound(64, 64), 65_536 + 4_096 * 512);
    assert_eq!(Awaited::load().header_bound(), 65_536);
    assert_eq!(Awaited::unload().header_bound(), 65_536);
    // No request allows more than ADR 0006's most, whatever a caller says of it.
    assert_eq!(bound(65, 64), bound(64, 64));
    assert_eq!(bound(64, 65), bound(64, 64));
    assert_eq!(bound(u32::MAX, u32::MAX), 2_162_688);

    // The most instances ADR 0006 allows, 64 for each of 64 queries, in the smallest
    // picture: written, and read back under the bound of the request that allows them.
    let pixel = |query: u32, score: f64| Found {
        query,
        score,
        iou: Some(0.5),
        bounds: [0.0, 0.0, 1.0, 1.0],
        plane: Raster {
            binding: under,
            width: 1,
            height: 1,
            samples: vec![255],
        },
    };
    let mut all = Vec::new();
    for query in 0..QUERY_LIMIT as u32 {
        for at in 0..INSTANCE_LIMIT {
            all.push(pixel(query, 1.0 - f64::from(at) / 64.0));
        }
    }
    let opened = |queries: u32, instances: u32| {
        let mut open = Open::new();
        let awaited = Awaited::segment(1, 1, under, queries, instances);
        open.expect("r1", awaited).unwrap();
        open
    };
    let bytes = written(&result("r1", all.clone()));
    let header = u64::from(u32::from_be_bytes(bytes[..4].try_into().unwrap()));
    assert!(header > 4_096 * 300 && header <= bound(64, 64), "{header}");
    let Message::Result { answer, .. } = one(&bytes, &mut opened(64, 64)) else {
        panic!("not a result");
    };
    let Answer::Segment(read_back) = answer else {
        panic!("not a segment's");
    };
    assert_eq!(read_back.len(), 4_096);
    // One instance more than any request allows is not written at all.
    let mut one_more = all.clone();
    one_more.push(pixel(0, 0.0));
    assert_eq!(unwritten(&result("r1", one_more)), Reason::OverBound);
    // Under a request that allows one instance fewer for each query, the same result
    // is refused for its count; under one of half the queries it is over its header's
    // bound, and is refused from its four bytes.
    assert_eq!(refused(&bytes, &mut opened(64, 63)), Reason::OverBound);
    assert!(header > bound(32, 64));
    assert_eq!(
        refused(&bytes[..4], &mut opened(32, 64)),
        Reason::HeaderTooLarge
    );

    // The four bytes of a header's length are held to the largest bound among the
    // requests the reader has open: at it the reader goes on to a header that is not
    // there, and one byte over is refused.
    for (queries, instances) in [(1, 1), (3, 2), (64, 64)] {
        let bound = bound(queries, instances) as u32;
        let mut open = opened(queries, instances);
        assert_eq!(refused(&bound.to_be_bytes(), &mut open), Reason::Truncated);
        assert_eq!(
            refused(&(bound + 1).to_be_bytes(), &mut open),
            Reason::HeaderTooLarge
        );
    }
    // With two requests open the four bytes are held to the larger bound, and each
    // message is then held to its own: a result of a `load`, and an event, are held to
    // 64 KiB though a request that allows more is open beside them.
    let mut two = opened(64, 64);
    two.expect("r2", Awaited::load()).unwrap();
    let long = "x".repeat(65_500);
    let loaded = Message::Result {
        id: "r2".to_owned(),
        device: "cpu".to_owned(),
        threads: 3,
        diagnostics: long.clone(),
        answer: Answer::Load,
    };
    let mut bytes = Vec::new();
    let unbounded = proto::write(&loaded, &mut bytes);
    assert!(
        matches!(unbounded, Err(Unwritten::Refused(_))),
        "{unbounded:?}"
    );
    let by_hand = format!(
        "{{\"type\":\"result\",\"id\":\"r2\",\"kind\":\"load\",\"device\":\"cpu\",\
         \"threads\":3,\"diagnostics\":\"{long}\"}}"
    );
    assert!(by_hand.len() as u64 > HEADER_LIMIT);
    assert_eq!(
        refused(&frame(by_hand.as_bytes(), &[]), &mut two),
        Reason::HeaderTooLarge
    );
    let said = format!("{{\"type\":\"event\",\"id\":\"r1\",\"text\":\"{long}\"}}");
    assert_eq!(said.len() as u64, HEADER_LIMIT);
    assert!(
        read(&frame(said.as_bytes(), &[]), &mut two)
            .unwrap()
            .is_some()
    );
    let said = format!("{{\"type\":\"event\",\"id\":\"r2\",\"text\":\"{long}x\"}}");
    assert_eq!(
        refused(&frame(said.as_bytes(), &[]), &mut two),
        Reason::HeaderTooLarge
    );
    // A result's header over 64 KiB is read while a `load` is open beside its request,
    // whichever of the two ids is the lesser: its four bytes are held to the larger
    // bound, not the smaller and not the first. At that bound the reader goes on to a
    // header that is not there, and one byte over is refused.
    let bytes = written(&result("r1", all.clone()));
    assert!(header > HEADER_LIMIT);
    for beside in ["r0", "r2"] {
        let both = || {
            let mut open = opened(64, 64);
            open.expect(beside, Awaited::load()).unwrap();
            open
        };
        let read_back = one(&bytes, &mut both());
        assert!(matches!(read_back, Message::Result { .. }), "{beside}");
        let at = bound(64, 64) as u32;
        assert_eq!(refused(&at.to_be_bytes(), &mut both()), Reason::Truncated);
        assert_eq!(
            refused(&(at + 1).to_be_bytes(), &mut both()),
            Reason::HeaderTooLarge
        );
    }

    // Every other message is held to 64 KiB too, whatever is open. A batch of the most
    // phrases, each of the most bytes, fits; the same batch of characters that are
    // each written as two is over, and is refused where it is written and where it
    // is read.
    let asking = |character: &str| Message::Request {
        id: "r3".to_owned(),
        ask: Ask::Segment {
            model: model(),
            installation: installation(),
            image: image(under),
            queries: vec![Query::phrase(&character.repeat(512)).unwrap(); QUERY_LIMIT],
            max_instances: 1,
        },
    };
    let plain = written(&asking("p"));
    assert!(read(&plain, &mut two).unwrap().is_some());
    assert_eq!(unwritten(&asking("\"")), Reason::HeaderTooLarge);
    let quoted: Vec<String> = (0..QUERY_LIMIT)
        .map(|_| format!("{{\"phrase\":\"{}\"}}", "\\\"".repeat(512)))
        .collect();
    let text = String::from_utf8(plain[4..plain.len() - 8 - 192].to_vec()).unwrap();
    let start = text.find("\"queries\":[").unwrap() + 11;
    let end = text.find("],\"max_instances\"").unwrap();
    let escaped = format!("{}{}{}", &text[..start], quoted.join(","), &text[end..]);
    assert!(escaped.len() as u64 > HEADER_LIMIT);
    assert_eq!(
        refused(&frame(escaped.as_bytes(), &[]), &mut two),
        Reason::HeaderTooLarge
    );
    assert_eq!(
        refused(&frame(escaped.as_bytes(), &[]), &mut Open::new()),
        Reason::HeaderTooLarge
    );

    // An entry is at most 512 bytes: the longest one there can be, of a plane whose
    // width and height take the most digits 50,000,000 pixels allow, with every number
    // at its longest. The reader takes the header and goes on to a plane that is not
    // there.
    let large = |query: u32| Found {
        query,
        score: 0.123_456_789,
        iou: Some(0.5),
        bounds: [0.1, 0.2, 0.3, 0.4],
        plane: Raster {
            binding: under,
            width: 10_000_000,
            height: 5,
            samples: Vec::new(),
        },
    };
    let longest = entry(&large(63));
    assert!(longest.len() <= 512, "{}", longest.len());
    assert_eq!(longest.len(), 330);
    let mut open = Open::new();
    let awaited = Awaited::segment(10_000_000, 5, under, 64, 64);
    open.expect("r1", awaited).unwrap();
    let header = result_header("r1", "", &[longest]);
    assert_eq!(refused(&frame(&header, &[]), &mut open), Reason::Truncated);
    // So a result that keeps every other bound fits its header's: the most text, and
    // the one instance its request allows.
    let most = "x".repeat(DIAGNOSTICS_LIMIT as usize);
    let full = result_header("r1", &most, &[entry(&large(0))]);
    let mut open = Open::new();
    let awaited = Awaited::segment(10_000_000, 5, under, 1, 1);
    open.expect("r1", awaited).unwrap();
    assert!(full.len() as u64 <= bound(1, 1), "{}", full.len());
    assert_eq!(refused(&frame(&full, &[]), &mut open), Reason::Truncated);
}

#[test]
fn a_load_carries_at_most_32_settings_and_one_more_is_refused() {
    assert_eq!(SETTING_LIMIT, 32);
    let load = |count: usize| {
        let settings = (0..count)
            .map(|at| (format!("setting_{at:02}"), Scalar::Whole(at as i64)))
            .collect();
        Message::Request {
            id: "r1".to_owned(),
            ask: Ask::Load {
                model: model(),
                installation: installation(),
                settings,
            },
        }
    };
    for count in [0, 1, 32] {
        let message = load(count);
        assert_eq!(
            one(&written(&message), &mut Open::new()),
            message,
            "{count}"
        );
    }
    // One more is refused by the writer, which writes nothing, and by the reader.
    assert_eq!(unwritten(&load(33)), Reason::OverBound);
    let (header, _) = taken_apart(&written(&load(32)));
    let item = |at: usize| format!("{{\"name\":\"setting_{at:02}\",\"value\":{at}}}");
    let by_hand = |count: usize| {
        let items: Vec<String> = (0..count).map(item).collect();
        let header = format!(
            "{{\"type\":\"request\",\"id\":\"r1\",\"kind\":\"load\",\"model\":\"fake-ok\",\
             \"installation\":\"{}\",\"settings\":[{}]}}",
            installation(),
            items.join(",")
        );
        frame(header.as_bytes(), &[])
    };
    assert_eq!(by_hand(32), written(&load(32)));
    assert_eq!(header["settings"].as_array().unwrap().len(), 32);
    assert_eq!(refused(&by_hand(33), &mut Open::new()), Reason::OverBound);

    // A value is a scalar as the config has one: a text, a whole number of 64 bits
    // with a sign, or true or false.
    let with = |value: Value| changed(&load(1), |header| header["settings"][0]["value"] = value);
    let taken = |value: Value| {
        let frame = with(value);
        // The changed header's fields are in the order of their names, so it is read
        // as far as its values and refused only for its writing.
        refused(&frame, &mut Open::new())
    };
    for value in [json!(i64::MIN), json!(i64::MAX), json!("0.2"), json!(false)] {
        assert_eq!(taken(value.clone()), Reason::NotCanonical, "{value}");
    }
    let wrong = [
        json!(0.2),
        json!(u64::MAX),
        json!(null),
        json!([1]),
        json!({"a": 1}),
        json!(""),
        json!(" padded"),
        json!("padded "),
        json!("two\nlines"),
        json!("caf\u{e9}"),
        json!("x".repeat(1_025)),
    ];
    for value in wrong {
        assert_eq!(taken(value.clone()), Reason::BadValue, "{value}");
    }
    assert_eq!(taken(json!("x".repeat(1_024))), Reason::NotCanonical);
    // A name is a setting's key as the config has one, and is given once, in the
    // order of the names' bytes.
    let named = |names: &[&str]| {
        let items: Vec<String> = names
            .iter()
            .map(|name| format!("{{\"name\":\"{name}\",\"value\":1}}"))
            .collect();
        let header = format!(
            "{{\"type\":\"request\",\"id\":\"r1\",\"kind\":\"load\",\"model\":\"fake-ok\",\
             \"installation\":\"{}\",\"settings\":[{}]}}",
            installation(),
            items.join(",")
        );
        read(&frame(header.as_bytes(), &[]), &mut Open::new()).map_err(|said| said.reason())
    };
    assert!(named(&["a", "b", "b_2"]).is_ok());
    assert_eq!(named(&["b", "a"]), Err(Reason::OutOfOrder));
    assert_eq!(named(&["a", "a"]), Err(Reason::OutOfOrder));
    let long = "n".repeat(65);
    for name in ["", "A", "1a", "a-b", "a.b", "_a", long.as_str()] {
        assert_eq!(named(&[name]), Err(Reason::BadValue), "{name}");
    }
    assert!(named(&[&"n".repeat(64)]).is_ok());
}

#[test]
fn a_result_over_one_gibibyte_of_payload_is_refused_whatever_its_request_would_allow() {
    let under = binding("a picture");
    assert_eq!(RESULT_PAYLOAD_LIMIT, 1 << 30);
    // A picture of 2^25 pixels, and a request that allows 64 instances of it: two
    // gibibytes of planes. The headers are written by hand, and no plane follows them.
    let (width, height) = (32_768, 1_024);
    let large = || Found {
        query: 0,
        score: 0.5,
        iou: None,
        bounds: [0.0, 0.0, 1.0, 1.0],
        plane: Raster {
            binding: under,
            width,
            height,
            samples: Vec::new(),
        },
    };
    let answer = |count: usize| {
        let one = entry(&large());
        frame(&result_header("r1", "", &vec![one; count]), &[])
    };
    let opened = |queries: u32, instances: u32| {
        let mut open = Open::new();
        let awaited = Awaited::segment(width, height, under, queries, instances);
        open.expect("r1", awaited).unwrap();
        open
    };
    // 32 planes are exactly a gibibyte: the reader goes on to a plane that is not
    // there, and has set nothing aside for it.
    assert_eq!(32 * u64::from(width) * u64::from(height), 1 << 30);
    assert_eq!(refused(&answer(32), &mut opened(1, 64)), Reason::Truncated);
    // One more is over, whatever the request allows.
    assert_eq!(refused(&answer(33), &mut opened(1, 64)), Reason::TooLarge);
    assert_eq!(refused(&answer(64), &mut opened(1, 64)), Reason::TooLarge);
    // The count a request allows is still held: 33 instances under a request for 32.
    assert_eq!(refused(&answer(33), &mut opened(1, 32)), Reason::OverBound);
    // The reader's bound is to the byte: 25 planes of 42,949,673 pixels are one byte
    // over a gibibyte, and are refused from the header, though nothing follows it.
    let odd: u64 = 42_949_673;
    assert_eq!(25 * odd, (1 << 30) + 1);
    let one_over = Found {
        plane: Raster {
            binding: under,
            width: odd as u32,
            height: 1,
            samples: Vec::new(),
        },
        ..large()
    };
    let header = result_header("r1", "", &vec![entry(&one_over); 25]);
    let mut open = Open::new();
    let awaited = Awaited::segment(odd as u32, 1, under, 1, 25);
    open.expect("r1", awaited).unwrap();
    assert_eq!(refused(&frame(&header, &[]), &mut open), Reason::TooLarge);
    // And the writer refuses to write such a result, before it writes a byte.
    let found = large();
    assert_eq!(
        unwritten(&result("r1", vec![found.clone(); 33])),
        Reason::TooLarge
    );
    // The writer holds a result to the same gibibyte, to the byte: 32 such planes pass
    // it, and are refused only because these have no samples; one byte more, a plane
    // of one pixel beside them, is over.
    let mut exact = vec![found; 32];
    assert_eq!(unwritten(&result("r1", exact.clone())), Reason::WrongLength);
    exact.push(Found {
        query: 0,
        score: 0.25,
        iou: None,
        bounds: [0.0, 0.0, 1.0, 1.0],
        plane: Raster {
            binding: under,
            width: 1,
            height: 1,
            samples: vec![255],
        },
    });
    assert_eq!(unwritten(&result("r1", exact)), Reason::TooLarge);
}

// --- what is refused --------------------------------------------------------------------------

/// Every object of a header, by the place it is at: the header itself, and each
/// object inside it.
fn objects(header: &Value) -> Vec<String> {
    let mut places = Vec::new();
    let mut left = vec![(String::new(), header)];
    while let Some((at, value)) = left.pop() {
        match value {
            Value::Object(fields) => {
                places.push(at.clone());
                for (name, value) in fields {
                    left.push((format!("{at}/{name}"), value));
                }
            }
            Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    left.push((format!("{at}/{index}"), item));
                }
            }
            _ => {}
        }
    }
    places
}

#[test]
fn an_unknown_field_a_missing_field_and_an_unknown_message_are_each_refused() {
    let (mut unknown, mut missing) = (0, 0);
    for (name, message, open) in every_message() {
        let (header, _) = taken_apart(&written(&message));
        let rebuilt = |open: &Open| {
            let mut again = Open::new();
            if let Some(awaited) = open.awaited("r1") {
                again.expect("r1", awaited.clone()).unwrap();
            }
            again
        };
        for place in objects(&header) {
            // A field that is not the object's own.
            let extra = changed(&message, |header| {
                header.pointer_mut(&place).unwrap()["extra"] = json!(1);
            });
            assert_eq!(
                refused(&extra, &mut rebuilt(&open)),
                Reason::UnknownField,
                "{name} {place}"
            );
            unknown += 1;
            // Each field of the object's own, taken away.
            let fields: Vec<String> = header
                .pointer(&place)
                .unwrap()
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect();
            for field in fields {
                let without = changed(&message, |header| {
                    let object = header.pointer_mut(&place).unwrap();
                    object.as_object_mut().unwrap().remove(&field);
                });
                assert_eq!(
                    refused(&without, &mut rebuilt(&open)),
                    Reason::MissingField,
                    "{name} {place} without {field}"
                );
                missing += 1;
            }
        }
        // A message of no type `pack/1` has, and a type that is no text.
        for other in ["ping", "", "Hello", "requests"] {
            let named = changed(&message, |header| header["type"] = json!(other));
            assert_eq!(
                refused(&named, &mut rebuilt(&open)),
                Reason::UnknownMessage,
                "{name} as {other}"
            );
        }
        let number = changed(&message, |header| header["type"] = json!(7));
        assert_eq!(refused(&number, &mut rebuilt(&open)), Reason::BadValue);
        // A request or a result of a kind `pack/1` has not.
        if header.get("kind").is_some() {
            let kind = changed(&message, |header| header["kind"] = json!("fill"));
            assert_eq!(refused(&kind, &mut rebuilt(&open)), Reason::UnknownKind);
        }
    }
    assert!(unknown >= 20 && missing >= 80, "{unknown} and {missing}");

    // A header that is not one JSON object, or gives a field twice.
    let not_json: [&[u8]; 8] = [
        b"[]",
        b"\"hello\"",
        b"{",
        b"{\"type\":\"cancel\",\"id\":\"r1\"}}",
        b"{\"type\":\"cancel\",\"id\":\"r1\",\"id\":\"r1\"}",
        b"{\"type\":\"cancel\",\"id\":NaN}",
        b"\xef\xbb\xbf{\"type\":\"cancel\",\"id\":\"r1\"}",
        b"{\"type\":\"cancel\",\"id\":\"r\xff\"}",
    ];
    for header in not_json {
        assert_eq!(
            refused(&frame(header, &[]), &mut Open::new()),
            Reason::NotJson,
            "{}",
            String::from_utf8_lossy(header)
        );
    }
    // A header that is the right message in another writing.
    let not_canonical: [&[u8]; 5] = [
        b"{\"type\":\"cancel\", \"id\":\"r1\"}",
        b"{\"id\":\"r1\",\"type\":\"cancel\"}",
        b"{\"type\":\"cancel\",\"id\":\"r\\u0031\"}",
        b"{\"type\":\"cancel\",\"id\":\"r1\"}\n",
        b" {\"type\":\"cancel\",\"id\":\"r1\"}",
    ];
    for header in not_canonical {
        assert_eq!(
            refused(&frame(header, &[]), &mut Open::new()),
            Reason::NotCanonical,
            "{}",
            String::from_utf8_lossy(header)
        );
    }
    let canonical = b"{\"type\":\"cancel\",\"id\":\"r1\"}";
    assert!(read(&frame(canonical, &[]), &mut Open::new()).is_ok());

    // A result or an event for a request the reader has not open; a result of another
    // kind than its request; and a second result for one request.
    let under = binding("a picture");
    let done = written(&result("r1", Vec::new()));
    assert_eq!(refused(&done, &mut Open::new()), Reason::Unexpected);
    let mut other = Open::new();
    other.expect("r1", Awaited::load()).unwrap();
    assert_eq!(refused(&done, &mut other), Reason::Unexpected);
    let mut open = awaiting("r1", under);
    assert!(read(&done, &mut open).unwrap().is_some());
    assert!(open.awaited("r1").is_none());
    assert_eq!(refused(&done, &mut open), Reason::Unexpected);
    let said = written(&event("r1", "late"));
    assert_eq!(refused(&said, &mut open), Reason::Unexpected);
    // An error ends its request too.
    let mut open = awaiting("r1", under);
    let failed = Message::Error {
        id: "r1".to_owned(),
        fault: FaultKind::Cancelled,
        sentence: "the call was cancelled".to_owned(),
    };
    assert_eq!(one(&written(&failed), &mut open), failed);
    assert!(open.awaited("r1").is_none());
    // And an id is open once.
    let mut open = awaiting("r1", under);
    let again = open.expect("r1", Awaited::load()).unwrap_err();
    assert_eq!(again.reason(), Reason::Unexpected);
}

#[test]
fn a_value_that_breaks_its_fields_rule_is_refused() {
    let under = binding("a picture");
    let refuses = |message: &Message, place: &str, value: Value, open: &Open| {
        let frame = changed(message, |header| {
            *header.pointer_mut(place).unwrap() = value.clone();
        });
        let mut again = Open::new();
        if let Some(awaited) = open.awaited("r1") {
            again.expect("r1", awaited.clone()).unwrap();
        }
        match read(&frame, &mut again) {
            Err(said) => said.reason(),
            Ok(_) => panic!("{place} took {value}"),
        }
    };
    let messages: BTreeMap<&str, (Message, Open)> = every_message()
        .into_iter()
        .map(|(name, message, open)| (name, (message, open)))
        .collect();
    let bad = |name: &str, place: &str, value: Value| {
        let (message, open) = &messages[name];
        assert_eq!(
            refuses(message, place, value.clone(), open),
            Reason::BadValue,
            "{name} {place} {value}"
        );
    };
    let over = |name: &str, place: &str, value: Value| {
        let (message, open) = &messages[name];
        assert_eq!(
            refuses(message, place, value.clone(), open),
            Reason::OverBound,
            "{name} {place} {value}"
        );
    };
    // An id.
    let long = "r".repeat(65);
    for id in ["", "R1", "-r", "r 1", "r/1", long.as_str()] {
        bad("cancel", "/id", json!(id));
    }
    bad("cancel", "/id", json!(1));
    // A hello.
    bad("hello-worker", "/role", json!("server"));
    for device in ["", "CPU", "0", "cuda 0", "cpu/0", &"d".repeat(33)] {
        bad("hello-worker", "/device", json!(device));
        bad("result-load", "/device", json!(device));
    }
    bad("result-load", "/device", json!(null));
    for threads in [json!(0), json!(65_536), json!(-1), json!(1.5), json!("3")] {
        bad("hello-worker", "/threads", threads.clone());
        bad("result-load", "/threads", threads);
    }
    // A request.
    for name in ["", "Fake", "fake ok", "../fake", &"m".repeat(65)] {
        bad("request-load", "/model", json!(name));
    }
    for identity in ["", "sha256:", "abc", &format!("sha256:{}", "A".repeat(64))] {
        bad("request-load", "/installation", json!(identity));
        bad("request-segment", "/image/binding", json!(identity));
    }
    bad("request-load", "/settings", json!({}));
    bad("request-segment", "/max_instances", json!(0));
    over("request-segment", "/max_instances", json!(65));
    bad("request-segment", "/queries", json!([]));
    over(
        "request-segment",
        "/queries",
        json!(vec![json!({"phrase": "a dog"}); 65]),
    );
    bad("request-segment", "/queries/0", json!("a dog"));
    bad("request-segment", "/queries/0/phrase", json!(""));
    over(
        "request-segment",
        "/queries/0/phrase",
        json!("p".repeat(513)),
    );
    bad(
        "request-segment",
        "/queries/0",
        json!({"phrase": "a dog", "point": [bits(1.0), bits(1.0)]}),
    );
    // A number with a fraction is 16 lower-case hex digits, and is finite.
    let numbers = [
        json!(1.5),
        json!("1.5"),
        json!("3FF8000000000000"),
        json!("3ff80000000000"),
        json!(bits(f64::NAN)),
        json!(bits(f64::INFINITY)),
        json!(bits(f64::NEG_INFINITY)),
    ];
    for number in numbers {
        bad("request-segment", "/queries/1/point/0", number.clone());
        bad("request-segment", "/queries/2/box/3", number.clone());
        bad("result-segment", "/instances/0/score", number.clone());
        bad("result-segment", "/instances/0/iou", number.clone());
        bad("result-segment", "/instances/0/box/0", number);
    }
    bad("request-segment", "/queries/1/point", json!([bits(1.0)]));
    bad("request-segment", "/queries/1/point/0", json!(bits(-1.0)));
    bad(
        "request-segment",
        "/queries/2/box",
        json!([bits(3.0), bits(2.0), bits(1.0), bits(4.5)]),
    );
    // A result.
    for score in [1.000_001, -0.1] {
        bad("result-segment", "/instances/0/score", json!(bits(score)));
        bad("result-segment", "/instances/0/iou", json!(bits(score)));
    }
    bad("result-segment", "/instances/0/query", json!(-1));
    bad("result-segment", "/instances/0/box", json!([]));
    bad("result-segment", "/instances", json!({}));
    // A text of the protocol holds what a fault's sentence may.
    for text in [
        "a/b",
        "a\\b",
        "two\nlines",
        "\"quoted\"",
        "caf\u{e9}",
        "a=b",
    ] {
        bad("event", "/text", json!(text));
        bad("result-load", "/diagnostics", json!(text));
        bad("error", "/sentence", json!(text));
    }
    bad("event", "/text", json!(""));
    bad("error", "/sentence", json!(""));
    bad("error", "/sentence", json!("s".repeat(241)));
    bad("error", "/fault", json!("broken"));
    bad("error", "/fault", json!("Cancelled"));
    // Every fault ADR 0006 names is one an error may carry.
    for fault in FaultKind::ALL {
        let message = Message::Error {
            id: "r1".to_owned(),
            fault,
            sentence: "s".repeat(240),
        };
        assert_eq!(one(&written(&message), &mut awaiting("r1", under)), message);
    }
    // The writer refuses what the reader would.
    let wrong = [
        Message::Cancel {
            id: "R1".to_owned(),
        },
        event("r1", "a/b"),
        event("r1", ""),
        Message::Hello {
            id: "h1".to_owned(),
            role: Role::Worker {
                device: Some("CPU".to_owned()),
                threads: 3,
            },
        },
        Message::Hello {
            id: "h1".to_owned(),
            role: Role::Worker {
                device: None,
                threads: 0,
            },
        },
        Message::Error {
            id: "r1".to_owned(),
            fault: FaultKind::Malformed,
            sentence: "s".repeat(241),
        },
        result("r1", vec![found(under, 0, 1.5, [0, 0, 1, 1])]),
        result("r1", vec![found(under, 0, f64::NAN, [0, 0, 1, 1])]),
    ];
    for message in wrong {
        assert_eq!(unwritten(&message), Reason::BadValue, "{message:?}");
    }
}

/// An output that takes this many bytes and then fails.
struct Failing(usize);

impl std::io::Write for Failing {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.0 == 0 {
            return Err(std::io::ErrorKind::BrokenPipe.into());
        }
        let taken = bytes.len().min(self.0);
        self.0 -= taken;
        Ok(taken)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn an_output_that_fails_is_told_apart_from_a_message_that_is_refused() {
    let under = binding("a picture");
    let request = segment("r1", under);
    let whole = written(&request).len();
    // Wherever the output fails, in the length, the header or the payload, the
    // writer says that the output failed and with what; it never says the message
    // was at fault.
    for taken in [0, 2, 4, 100, whole - 1] {
        let failed = proto::write(&request, &mut Failing(taken));
        let expected = Unwritten::Output(std::io::ErrorKind::BrokenPipe);
        assert_eq!(failed, Err(expected), "{taken}");
    }
    assert_eq!(proto::write(&request, &mut Failing(whole)), Ok(()));
    // An input that fails inside a frame, or before one, is a cut frame: it gives the
    // frame's bytes one at a time, and then fails where it would have ended.
    struct Broken(Vec<u8>, usize);
    impl std::io::Read for Broken {
        fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
            if self.1 == self.0.len() {
                return Err(std::io::ErrorKind::BrokenPipe.into());
            }
            bytes[0] = self.0[self.1];
            self.1 += 1;
            Ok(1)
        }
    }
    let frame = written(&request);
    for given in [0, 2, 4, 100, whole - 1] {
        let mut input = Broken(frame[..given].to_vec(), 0);
        let failed = proto::read(&mut input, &mut Open::new());
        assert_eq!(failed.unwrap_err().reason(), Reason::Truncated, "{given}");
    }
    // Given whole, a byte at a time, the frame is read.
    let mut input = Broken(frame, 0);
    assert_eq!(proto::read(&mut input, &mut Open::new()), Ok(Some(request)));
}

// --- payloads -----------------------------------------------------------------------------------

#[test]
fn a_payload_states_its_dimensions_layout_type_colour_alpha_and_binding_and_one_that_disagrees_with_its_length_is_refused()
 {
    let under = binding("a picture");
    // The picture of a request.
    let request = segment("r1", under);
    let (header, payloads) = taken_apart(&written(&request));
    let expected = json!({
        "width": WIDTH, "height": HEIGHT, "layout": "rgba", "data": "u8",
        "colour": "srgb", "alpha": "straight", "binding": under.to_string(),
    });
    assert_eq!(header["image"], expected);
    assert_eq!(under.to_string().len(), 71);
    // Its samples follow the header as they are, after the eight bytes of their count.
    let count = (4 * WIDTH * HEIGHT) as u64;
    assert_eq!(payloads[..8], count.to_be_bytes());
    assert_eq!(payloads[8..], rgba());
    // The plane of an instance.
    let answer = result("r1", vec![found(under, 0, 0.9, [2, 1, 5, 4])]);
    let (header, payloads) = taken_apart(&written(&answer));
    let expected = json!({
        "width": WIDTH, "height": HEIGHT, "layout": "plane", "data": "u8",
        "colour": "none", "alpha": "none", "binding": under.to_string(),
    });
    assert_eq!(header["instances"][0]["plane"], expected);
    assert_eq!(payloads[..8], u64::from(WIDTH * HEIGHT).to_be_bytes());
    assert_eq!(payloads[8..], plane(under, [2, 1, 5, 4]).samples);

    // No other layout, type or rule exists, and a plane does not take a picture's.
    let places = [(&request, "/image"), (&answer, "/instances/0/plane")];
    for (message, place) in places {
        let others = [
            ("layout", "rgb"),
            ("layout", if place == "/image" { "plane" } else { "rgba" }),
            ("data", "u16"),
            ("data", "f32"),
            ("colour", "linear"),
            ("colour", if place == "/image" { "none" } else { "srgb" }),
            ("alpha", "premultiplied"),
            (
                "alpha",
                if place == "/image" {
                    "none"
                } else {
                    "straight"
                },
            ),
        ];
        for (field, other) in others {
            let frame = changed(message, |header| {
                header.pointer_mut(place).unwrap()[field] = json!(other);
            });
            assert_eq!(
                refused(&frame, &mut awaiting("r1", under)),
                Reason::BadValue,
                "{place} {field} {other}"
            );
        }
    }

    // A payload whose length is not what its entry gives. The writer refuses to write
    // one, and writes nothing.
    for change in [-1_i64, 1] {
        let mut short = image(under);
        short.samples.resize((count as i64 + change) as usize, 0);
        let message = Message::Request {
            id: "r1".to_owned(),
            ask: Ask::Segment {
                model: model(),
                installation: installation(),
                image: short,
                queries: queries(),
                max_instances: 2,
            },
        };
        assert_eq!(unwritten(&message), Reason::WrongLength);
        let mut wrong = found(under, 0, 0.9, [2, 1, 5, 4]);
        let length = (i64::from(WIDTH * HEIGHT) + change) as usize;
        wrong.plane.samples.resize(length, 0);
        assert_eq!(unwritten(&result("r1", vec![wrong])), Reason::WrongLength);
    }
    // And the reader refuses one, whichever of its two statements is the wrong one:
    // the eight bytes, or the entry.
    let whole = written(&request);
    let length = 4 + u32::from_be_bytes(whole[..4].try_into().unwrap()) as usize;
    for stated in [count - 1, count + 1] {
        let mut bytes = whole.clone();
        bytes[length..length + 8].copy_from_slice(&stated.to_be_bytes());
        assert_eq!(refused(&bytes, &mut Open::new()), Reason::WrongLength);
    }
    let text = String::from_utf8(whole[4..length].to_vec()).unwrap();
    let taller = text.replace("\"height\":6", "\"height\":7");
    assert_ne!(taller, text);
    let mut bytes = frame(taller.as_bytes(), &[]);
    bytes.extend(&whole[length..]);
    assert_eq!(refused(&bytes, &mut Open::new()), Reason::WrongLength);
    // The second plane of a result, too.
    let two = result(
        "r1",
        vec![
            found(under, 0, 0.9, [2, 1, 5, 4]),
            found(under, 1, 0.8, [0, 0, 2, 2]),
        ],
    );
    let mut bytes = written(&two);
    let second = bytes.len() - (WIDTH * HEIGHT) as usize - 8;
    bytes[second + 7] += 1;
    assert_eq!(
        refused(&bytes, &mut awaiting("r1", under)),
        Reason::WrongLength
    );
    // What a `Raster` prints is its size and what it belongs to, never its samples.
    assert_eq!(
        format!("{:?}", image(under)),
        format!("Raster(8 by 6, 192 bytes, {under})")
    );
}

/// A picture of `WIDTH` by `HEIGHT`, as the core holds one.
fn picture() -> Picture {
    Picture::new(CanonicalPixels::from_rgba(WIDTH, HEIGHT, rgba()).unwrap()).unwrap()
}

/// A segmenter that answers through the codec, in memory: it writes its request as a
/// frame, reads it back as a worker would, answers every query with one instance whose
/// plane names the binding the request it read names, writes that as a frame, and
/// reads it under what it has open.
struct Through {
    declaration: Declaration,
    /// The binding the planes are made to name, when it is not the request's own.
    naming: Option<BindingId>,
}

impl Through {
    fn new(naming: Option<BindingId>) -> Arc<Through> {
        let model = ModelIdentity::new("pack", "fake-ok", "1");
        let declaration = Declaration::new(model, &[feature::PHRASE]).unwrap();
        Arc::new(Through {
            declaration,
            naming,
        })
    }

    fn asked(&self, request: &SegmentRequest) -> Result<Vec<Found>, Refused> {
        let ask = Ask::segment(request, model(), installation());
        let mut open = Open::new();
        open.expect("r1", Awaited::of(&ask))?;
        let sent = written(&Message::Request {
            id: "r1".to_owned(),
            ask,
        });
        // The worker's side: what it read is all it knows of the request.
        let Some(Message::Request { id, ask }) = read(&sent, &mut Open::new())? else {
            panic!("not a request");
        };
        let Ask::Segment { image, queries, .. } = ask else {
            panic!("not a segment");
        };
        let under = self.naming.unwrap_or(image.binding);
        let found = (0..queries.len() as u32)
            .map(|query| found(under, query, 0.9, [2, 1, 5, 4]))
            .collect();
        let answered = written(&result(&id, found));
        match read(&answered, &mut open)? {
            Some(Message::Result {
                answer: Answer::Segment(found),
                ..
            }) => Ok(found),
            other => panic!("not a segment's result: {other:?}"),
        }
    }
}

impl Segmenter for Through {
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn segment(&self, request: &SegmentRequest, _: &Bounds) -> Result<SegmentResponse, Fault> {
        let found = self
            .asked(request)
            .map_err(|refused| Fault::new(FaultKind::Malformed, refused.sentence()))?;
        let answers = proto::answers(found, request.queries().len())?;
        Ok(SegmentResponse::new(answers))
    }
}

#[test]
fn two_requests_of_the_same_pixels_under_two_bindings_are_two_requests_and_each_answer_names_its_own()
 {
    // One picture, under its own binding and placed onto another image.
    let dog = vec![Query::phrase("a dog").unwrap()];
    let own = SegmentRequest::new(picture(), dog, 2).unwrap();
    let space = Space::flat(WIDTH, HEIGHT).unwrap();
    let onto = PixelDigest(Digest::of_bytes(b"another image"));
    let from = Direction::SourceToDestination;
    let placed = Binding::new(
        picture().digest(),
        space,
        onto,
        space,
        from,
        Mapping::Identity,
    )
    .unwrap();
    let under = own.clone().under(&placed).unwrap();
    assert_eq!(own.image(), under.image());
    assert_ne!(own.binding(), under.binding());

    // They are two requests on the wire: the frames differ, in the entry's binding and
    // nowhere else, and each is read back under its own.
    let frame_of = |request: &SegmentRequest| {
        written(&Message::Request {
            id: "r1".to_owned(),
            ask: Ask::segment(request, model(), installation()),
        })
    };
    let (first, second) = (frame_of(&own), frame_of(&under));
    assert_ne!(first, second);
    assert_eq!(first.len(), second.len());
    let (mut one_header, one_rest) = taken_apart(&first);
    let (mut two_header, two_rest) = taken_apart(&second);
    assert_eq!(
        one_header["image"]["binding"],
        json!(own.binding().to_string())
    );
    assert_eq!(
        two_header["image"]["binding"],
        json!(under.binding().to_string())
    );
    one_header["image"]["binding"] = json!(null);
    two_header["image"]["binding"] = json!(null);
    assert_eq!((one_header, one_rest), (two_header, two_rest));
    for request in [&own, &under] {
        let Some(Message::Request { ask, .. }) =
            read(&frame_of(request), &mut Open::new()).unwrap()
        else {
            panic!("not a request");
        };
        let Ask::Segment { image, .. } = &ask else {
            panic!("not a segment");
        };
        assert_eq!(image.binding, request.binding());
        assert_eq!(image.samples, request.image().rgba());
        assert_eq!(Awaited::of(&ask), {
            Awaited::segment(WIDTH, HEIGHT, request.binding(), 1, 2)
        });
    }

    // Each answer names its own, through the codec and the handle.
    let handle = |naming| SegmentHandle::new(Through::new(naming), Cancellation::new());
    for request in [&own, &under] {
        let response = handle(None).segment(request).unwrap();
        assert_eq!(response.answers().len(), 1);
        assert_eq!(response.answers()[0][0].binding(), request.binding());
    }
    // An answer whose planes name the other request's binding is refused where it is
    // read, before a plane of it is.
    let crossed = [(&own, under.binding()), (&under, own.binding())];
    for (request, other) in crossed {
        let through = Through::new(Some(other));
        let said = through.asked(request).unwrap_err();
        assert_eq!(said.reason(), Reason::WrongBinding);
        let fault = handle(Some(other)).segment(request).unwrap_err();
        assert_eq!(fault.kind(), FaultKind::Malformed);
    }
    // With both open at once, each result is held to its own request's binding.
    let mut open = Open::new();
    for (id, request) in [("r1", &own), ("r2", &under)] {
        let ask = Ask::segment(request, model(), installation());
        open.expect(id, Awaited::of(&ask)).unwrap();
    }
    let naming = |id: &str, binding: BindingId| {
        written(&result(id, vec![found(binding, 0, 0.9, [2, 1, 5, 4])]))
    };
    assert_eq!(
        refused(&naming("r2", own.binding()), &mut open),
        Reason::WrongBinding
    );
    assert_eq!(
        refused(&naming("r1", under.binding()), &mut open),
        Reason::WrongBinding
    );
    assert!(
        read(&naming("r1", own.binding()), &mut open)
            .unwrap()
            .is_some()
    );
    assert!(
        read(&naming("r2", under.binding()), &mut open)
            .unwrap()
            .is_some()
    );
}

// --- beside a result ----------------------------------------------------------------------------

#[test]
fn the_257th_event_of_a_request_and_diagnostics_past_their_bound_are_refused_without_being_kept() {
    let under = binding("a picture");
    assert_eq!((EVENT_LIMIT, DIAGNOSTICS_LIMIT), (256, 65_536));
    let said = |open: &Open| {
        let awaited = open.awaited("r1").unwrap();
        (awaited.events(), awaited.text())
    };

    // 256 events are read; the 257th is refused, and is not counted or handed over.
    let mut open = awaiting("r1", under);
    let one_event = written(&event("r1", "step"));
    for count in 1..=EVENT_LIMIT {
        assert!(read(&one_event, &mut open).unwrap().is_some());
        assert_eq!(said(&open), (count, u64::from(count) * 4));
    }
    let more = read(&one_event, &mut open);
    assert_eq!(more.unwrap_err().reason(), Reason::OverBound);
    assert_eq!(said(&open), (256, 1_024));
    // Another request's events are counted apart.
    open.expect("r2", Awaited::load()).unwrap();
    assert!(
        read(&written(&event("r2", "step")), &mut open)
            .unwrap()
            .is_some()
    );
    assert_eq!(said(&open), (256, 1_024));

    // The text of a request's events is counted: 65,536 bytes are read, and one more
    // is refused and not kept.
    let mut open = awaiting("r1", under);
    let half = written(&event("r1", &"x".repeat(32_768)));
    assert!(read(&half, &mut open).unwrap().is_some());
    assert!(read(&half, &mut open).unwrap().is_some());
    assert_eq!(said(&open), (2, 65_536));
    let more = read(&written(&event("r1", "x")), &mut open);
    assert_eq!(more.unwrap_err().reason(), Reason::OverBound);
    assert_eq!(said(&open), (2, 65_536));

    // A result's diagnostics count with the events' text: up to the bound the result
    // is read, and one byte past it the result is refused and its request stays open.
    let with = |diagnostics: usize| {
        written(&Message::Result {
            id: "r1".to_owned(),
            device: "cpu".to_owned(),
            threads: 3,
            diagnostics: "d".repeat(diagnostics),
            answer: Answer::Segment(Vec::new()),
        })
    };
    let mut open = awaiting("r1", under);
    let most = written(&event("r1", &"x".repeat(65_000)));
    assert!(read(&most, &mut open).unwrap().is_some());
    let over = read(&with(537), &mut open);
    assert_eq!(over.unwrap_err().reason(), Reason::OverBound);
    assert_eq!(said(&open), (1, 65_000));
    assert!(read(&with(536), &mut open).unwrap().is_some());
    assert!(open.awaited("r1").is_none());
    // Alone, a result's diagnostics have the whole bound, and a text over it is
    // refused by the writer and by the reader.
    let mut open = awaiting("r1", under);
    assert!(read(&with(65_536), &mut open).unwrap().is_some());
    let long = result_header("r1", &"d".repeat(65_537), &[]);
    assert_eq!(
        refused(&frame(&long, &[]), &mut awaiting("r1", under)),
        Reason::OverBound
    );
    let message = Message::Result {
        id: "r1".to_owned(),
        device: "cpu".to_owned(),
        threads: 3,
        diagnostics: "d".repeat(65_537),
        answer: Answer::Segment(Vec::new()),
    };
    assert_eq!(unwritten(&message), Reason::OverBound);
    // What a vector's reader is told was already said is what it goes on from.
    let mut open = Open::new();
    let awaited = Awaited::load().after(255, 65_532);
    open.expect("r1", awaited).unwrap();
    assert!(read(&one_event, &mut open).unwrap().is_some());
    assert_eq!(said(&open), (256, 65_536));
}

// --- nothing is a path ---------------------------------------------------------------------------

/// Every field name of a header, at any depth.
fn names(header: &Value, into: &mut BTreeSet<String>) {
    match header {
        Value::Object(fields) => {
            for (name, value) in fields {
                into.insert(name.clone());
                names(value, into);
            }
        }
        Value::Array(items) => items.iter().for_each(|item| names(item, into)),
        _ => {}
    }
}

#[test]
fn no_message_has_a_field_that_holds_a_path() {
    // Every field any message has, from the vectors and from one of each message.
    let mut fields = BTreeSet::new();
    for (_, vector) in vectors() {
        if vector.get("refused").is_none() {
            let frame = hex(vector["frame"].as_str().unwrap());
            names(&taken_apart(&frame).0, &mut fields);
        }
    }
    let mut written_fields = BTreeSet::new();
    for (_, message, _) in every_message() {
        names(&taken_apart(&written(&message)).0, &mut written_fields);
    }
    assert_eq!(fields, written_fields);
    let expected = [
        "alpha",
        "binding",
        "box",
        "colour",
        "data",
        "device",
        "diagnostics",
        "fault",
        "height",
        "id",
        "image",
        "installation",
        "instances",
        "iou",
        "kind",
        "layout",
        "max_instances",
        "model",
        "name",
        "phrase",
        "plane",
        "point",
        "query",
        "queries",
        "role",
        "score",
        "sentence",
        "settings",
        "text",
        "threads",
        "type",
        "value",
        "version",
        "width",
    ];
    let expected: BTreeSet<String> = expected.into_iter().map(str::to_owned).collect();
    assert_eq!(fields, expected);
    // None is a place for a path, a folder or an address.
    for field in &fields {
        for word in [
            "path", "file", "dir", "folder", "root", "url", "uri", "address", "home",
        ] {
            assert!(!field.contains(word), "{field}");
        }
    }

    // Every text that one side fills in and the other acts on is refused when it
    // holds a separator. Each place below is a text field of a message, and between
    // them they are every text field but a phrase and the words that are one of a
    // closed list (`type`, `kind`, `role`, `version`, `fault`, and an entry's four).
    let under = binding("a picture");
    let places = [
        ("cancel", "/id"),
        ("hello-worker", "/id"),
        ("hello-worker", "/device"),
        ("request-load", "/model"),
        ("request-load", "/installation"),
        ("request-load", "/settings/0/name"),
        ("request-load", "/settings/0/value"),
        ("request-unload", "/model"),
        ("request-unload", "/installation"),
        ("request-segment", "/model"),
        ("request-segment", "/installation"),
        ("request-segment", "/image/binding"),
        ("result-load", "/device"),
        ("result-load", "/diagnostics"),
        ("result-segment", "/instances/0/plane/binding"),
        ("event", "/text"),
        ("error", "/sentence"),
    ];
    let messages: BTreeMap<&str, Message> = every_message()
        .into_iter()
        .map(|(name, message, _)| (name, message))
        .collect();
    let mut texts = BTreeSet::new();
    for (name, place) in places {
        let (header, _) = taken_apart(&written(&messages[name]));
        assert!(header.pointer(place).unwrap().is_string(), "{name} {place}");
        texts.insert(place.rsplit('/').next().unwrap().to_owned());
        let paths = [
            "/tmp/models",
            "models/sam3",
            "..\\models",
            "C:\\models",
            "/",
            "\\",
            "a/b",
        ];
        for path in paths {
            let frame = changed(&messages[name], |header| {
                *header.pointer_mut(place).unwrap() = json!(path);
            });
            let mut open = awaiting("r1", under);
            open.expect("r2", Awaited::load()).unwrap();
            assert_eq!(
                refused(&frame, &mut open),
                Reason::BadValue,
                "{name} {place} took {path}"
            );
        }
    }
    // The closed words and the phrase are the text fields that were left out.
    let closed = [
        "alpha", "colour", "data", "fault", "kind", "layout", "role", "type", "version",
    ];
    let mut every_text = BTreeSet::new();
    for (_, message, _) in every_message() {
        let (header, _) = taken_apart(&written(&message));
        let mut left = vec![(String::new(), &header)];
        while let Some((name, value)) = left.pop() {
            match value {
                Value::String(_) => {
                    every_text.insert(name);
                }
                Value::Object(fields) => {
                    left.extend(fields.iter().map(|(name, value)| (name.clone(), value)));
                }
                Value::Array(items) => {
                    left.extend(items.iter().map(|item| (name.clone(), item)));
                }
                _ => {}
            }
        }
    }
    // A number with a fraction is a text of hex digits, and holds no separator.
    for number in ["box", "iou", "point", "score"] {
        assert!(every_text.remove(number), "{number}");
    }
    for word in closed {
        assert!(every_text.remove(word), "{word}");
    }
    assert!(every_text.remove("phrase"));
    assert_eq!(every_text, texts);
    // A phrase is words for the model, and may hold any: it is the one text that is
    // not held to a rule, and nothing opens it.
    let asked = Message::Request {
        id: "r1".to_owned(),
        ask: Ask::Segment {
            model: model(),
            installation: installation(),
            image: image(under),
            queries: vec![Query::phrase("a black/white cat, \"sitting\"\n").unwrap()],
            max_instances: 1,
        },
    };
    assert_eq!(one(&written(&asked), &mut Open::new()), asked);
}

// --- the order of an answer ----------------------------------------------------------------------

/// Holds `ordered` to the fixed order under a request of these bounds: built in any
/// other order it is written as the same bytes, those bytes are read back and written
/// again as themselves, and with any two neighbours swapped by hand, in the header and
/// in the planes, they are refused.
fn held_to_the_fixed_order(ordered: &[Found], awaited: &Awaited) {
    let opened = || {
        let mut open = Open::new();
        open.expect("r1", awaited.clone()).unwrap();
        open
    };
    let expected = written(&result("r1", ordered.to_vec()));
    let read_back = one(&expected, &mut opened());
    assert_eq!(written(&read_back), expected);
    let Message::Result { answer, .. } = read_back else {
        panic!("not a result");
    };
    assert_eq!(answer, Answer::Segment(ordered.to_vec()));

    // Built in any other order, it is written as the same bytes: every rotation of the
    // list, and each backwards.
    for turn in 0..ordered.len() {
        let mut built = ordered.to_vec();
        built.rotate_left(turn);
        assert_eq!(written(&result("r1", built.clone())), expected, "{turn}");
        built.reverse();
        assert_eq!(written(&result("r1", built)), expected, "{turn} backwards");
    }

    // A result whose instances are in another order is refused where it is read: each
    // pair of neighbours, swapped by hand in the header and in the planes.
    let length = 4 + u32::from_be_bytes(expected[..4].try_into().unwrap()) as usize;
    let plane_bytes = 8 + (WIDTH * HEIGHT) as usize;
    for first in 0..ordered.len() - 1 {
        let mut swapped: Vec<usize> = (0..ordered.len()).collect();
        swapped.swap(first, first + 1);
        let entries: Vec<String> = swapped.iter().map(|at| entry(&ordered[*at])).collect();
        let mut bytes = frame(&result_header("r1", "", &entries), &[]);
        for at in &swapped {
            let start = length + at * plane_bytes;
            bytes.extend(&expected[start..start + plane_bytes]);
        }
        assert_eq!(
            refused(&bytes, &mut opened()),
            Reason::OutOfOrder,
            "{first}"
        );
    }
    // The same header with nothing swapped is the codec's own bytes.
    let entries: Vec<String> = ordered.iter().map(entry).collect();
    assert_eq!(
        frame(&result_header("r1", "", &entries), &[]),
        expected[..length]
    );
}

#[test]
fn an_answer_is_in_the_fixed_order_whatever_order_it_was_built_in() {
    let under = binding("a picture");
    let boxed = |query: u32, score: f64, bounds: [u32; 4]| found(under, query, score, bounds);
    // In the fixed order: best score first; then by the query; then by the box's top,
    // its left, its right and its bottom; then by the predicted IoU; then by the
    // plane's bytes.
    let mut same_box = boxed(1, 0.5, [1, 1, 3, 3]);
    same_box.plane.samples[9] = 0;
    let ordered = vec![
        boxed(2, 0.9, [4, 4, 6, 6]),
        boxed(0, 0.5, [4, 4, 6, 6]),
        boxed(1, 0.5, [1, 0, 3, 3]),
        boxed(1, 0.5, [0, 1, 3, 3]),
        boxed(1, 0.5, [1, 1, 2, 3]),
        boxed(1, 0.5, [1, 1, 3, 2]),
        same_box,
        boxed(1, 0.5, [1, 1, 3, 3]),
        boxed(0, 0.25, [0, 0, 1, 1]),
    ];
    let awaited = Awaited::segment(WIDTH, HEIGHT, under, 3, 8);
    held_to_the_fixed_order(&ordered, &awaited);

    // The order is total. Instances that tie on the score, the query and the box are
    // ordered by their predicted IoU, none first and then the lesser, before their
    // planes are looked at: a lesser IoU comes first though its plane's bytes are the
    // greater. And -0 is less than 0 wherever a number with a fraction is compared, so
    // a score of 0 is better than one of -0, and a box whose top is -0 comes before
    // one whose top is 0.
    let (lesser, greater) = (plane(under, [0, 0, 1, 1]), plane(under, [0, 0, 2, 2]));
    assert!(lesser.samples < greater.samples);
    let tied = |score: f64, top: f64, iou: Option<f64>, plane: &Raster| Found {
        query: 0,
        score,
        iou,
        bounds: [0.0, top, 1.0, 1.0],
        plane: plane.clone(),
    };
    let ties = vec![
        tied(0.125, 0.0, None, &greater),
        tied(0.125, 0.0, Some(-0.0), &greater),
        tied(0.125, 0.0, Some(0.0), &greater),
        tied(0.125, 0.0, Some(0.75), &lesser),
        tied(0.125, 0.0, Some(0.75), &greater),
        tied(0.0, -0.0, Some(0.75), &lesser),
        tied(0.0, 0.0, Some(0.75), &lesser),
        tied(-0.0, 0.0, Some(0.75), &lesser),
    ];
    held_to_the_fixed_order(&ties, &awaited);

    // What the core is handed keeps that order within each query.
    let answers = proto::answers(ordered, 3).unwrap();
    let scores = |query: usize| -> Vec<f64> {
        answers[query]
            .iter()
            .map(|instance| instance.score())
            .collect()
    };
    assert_eq!(scores(0), [0.5, 0.25]);
    assert_eq!(scores(1), [0.5; 6]);
    assert_eq!(scores(2), [0.9]);
    let tops: Vec<f64> = answers[1].iter().map(|i| i.bounds()[1]).collect();
    assert_eq!(tops, [0.0, 1.0, 1.0, 1.0, 1.0, 1.0]);
    // An instance for a query that was not asked is no answer.
    let stray = vec![boxed(3, 0.5, [0, 0, 1, 1])];
    assert!(proto::answers(stray, 3).is_err());
    // And a plane that is not one the core takes is refused there, with its rule.
    let mut grey = boxed(0, 0.5, [0, 0, 1, 1]);
    grey.plane.samples[0] = 127;
    assert!(proto::answers(vec![grey], 3).is_err());
}

// --- what the supervisor needs of the codec (task P3b) ----------------------------------------

fn error(sentence: &str) -> Message {
    Message::Error {
        id: "c1".to_owned(),
        fault: FaultKind::Malformed,
        sentence: sentence.to_owned(),
    }
}

#[test]
fn the_protocols_cleaner_puts_a_space_for_what_a_text_may_not_hold_and_never_a_question_mark() {
    // A space for each character a text of the protocol may not hold, white space at the
    // ends dropped, the first characters kept.
    let cases = [
        (
            "this side speaks pack/1 and the other said pack/2",
            240,
            "this side speaks pack 1 and the other said pack 2",
        ),
        ("a\\b\"c?d\ne\tf@g", 240, "a b c d e f g"),
        ("  /kept/  ", 240, "kept"),
        ("caf\u{e9} au lait", 240, "caf  au lait"),
        ("one two three", 7, "one two"),
        ("", 240, ""),
        ("/?\\", 240, ""),
        (
            "letters, digits 0-9; and . , ; : ' ( ) % + - _",
            240,
            "letters, digits 0-9; and . , ; : ' ( ) % + - _",
        ),
    ];
    for (text, most, cleaned) in cases {
        let made = proto::clean(text, most);
        assert_eq!(made, cleaned, "{text:?}");
        assert!(!made.contains('?'));
    }
    // An error's sentence: of 240 characters at most, and never empty.
    assert_eq!(proto::sentence("/?/"), "no reason was given");
    assert_eq!(proto::sentence(&"x".repeat(300)).len(), 240);
    // What it makes is written and read as a frame of pack/1. The core's cleaning of a
    // fault's sentence puts a `?`, which no text of the protocol holds: refused.
    let refused = "this side speaks pack/1 and the other said pack/2";
    let sentence = proto::sentence(refused);
    let mut open = Open::new();
    let read = proto::read(&mut Cursor::new(written(&error(&sentence))), &mut open);
    assert_eq!(read, Ok(Some(error(&sentence))));
    let core = Fault::new(FaultKind::Malformed, refused);
    assert!(core.sentence().contains('?'));
    let unwritten = proto::writable(&error(core.sentence())).unwrap_err();
    assert_eq!(unwritten.reason(), Reason::BadValue);
}

#[test]
fn a_request_heard_by_a_side_asked_nothing_is_handed_back_by_its_id_before_its_payload_is_read() {
    // A worker's request, as the supervisor's reader hears it: its header is read and
    // held to every rule, and not one byte of its payload. The input past its header and
    // its payload's length is not even there.
    let under = binding("a picture");
    let bytes = written(&segment("w1", under));
    let header = 4 + u64::from(u32::from_be_bytes(bytes[..4].try_into().unwrap()));
    let mut input = Cursor::new(bytes[..header as usize].to_vec());
    let mut open = Open::new();
    let heard = proto::hear(&mut input, &mut open);
    assert_eq!(
        heard,
        Ok(Some(proto::Heard::Asked {
            id: "w1".to_owned()
        }))
    );
    assert_eq!(input.position(), header);
    // A header that breaks a rule is refused as it would be by `read`.
    let mut bad = written(&segment("w1", under));
    bad[4] = b' ';
    let refused = proto::hear(&mut Cursor::new(bad), &mut Open::new()).unwrap_err();
    assert_eq!(refused.reason(), Reason::NotJson);
    // Every other message is read whole, as `read` reads it: a result under what is open.
    let mut open = awaiting("r1", under);
    let answer = result("r1", vec![found(under, 0, 0.5, [0, 0, 2, 2])]);
    let heard = proto::hear(&mut Cursor::new(written(&answer)), &mut open);
    assert_eq!(heard, Ok(Some(proto::Heard::Message(answer))));
    assert!(
        open.awaited("r1").is_none(),
        "the result closed its request"
    );
}

#[test]
fn a_refusal_of_a_count_of_a_results_instances_says_so_and_no_other_does() {
    // One instance more than its request allows for a query.
    let under = binding("a picture");
    let mut open = Open::new();
    open.expect("r1", Awaited::segment(WIDTH, HEIGHT, under, 1, 1))
        .unwrap();
    let two = result(
        "r1",
        vec![
            found(under, 0, 0.5, [0, 0, 2, 2]),
            found(under, 0, 0.25, [0, 0, 2, 2]),
        ],
    );
    let refused = proto::read(&mut Cursor::new(written(&two)), &mut open).unwrap_err();
    assert_eq!(refused.reason(), Reason::OverBound);
    assert!(refused.counts_instances());
    // Other bounds that are counted are not of instances: the events of a request.
    let mut open = Open::new();
    let awaited = Awaited::segment(WIDTH, HEIGHT, under, 3, 2).after(EVENT_LIMIT, 0);
    open.expect("r1", awaited).unwrap();
    let refused = proto::read(
        &mut Cursor::new(written(&event("r1", "one more"))),
        &mut open,
    )
    .unwrap_err();
    assert_eq!(refused.reason(), Reason::OverBound);
    assert!(!refused.counts_instances());
}
