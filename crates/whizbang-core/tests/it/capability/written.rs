//! A judge's turns and exchanges as data (ADR 0009): what is written is read back
//! through the checked constructors, strictly and within bounds; an opaque part is
//! written whole or as a digest, by the writer's choice; and a script is what a
//! scripted judge is read from.

use serde_json::{Value as Json, json};
use whizbang_core::capability::written::{
    EXCHANGE, Keep, SCRIPT, SCRIPT_ENTRIES_LIMIT, SCRIPT_MAX_BYTES, Unread, read_exchange,
    read_script, write_exchange, write_script,
};
use whizbang_core::capability::{
    Controls, Fault, FaultKind, JudgeRequest, Message, Opaque, Part, Picture, Role, StopReason,
    TEXT_LIMIT, ToolCall, ToolDefinition, ToolResult, Turn, Unfit, Usage,
};
use whizbang_core::identity::{CanonicalPixels, Digest, PixelDigest};

/// A body no JSON library would write: its keys out of order, a number with a
/// trailing zero, a space after a colon, and a character outside ASCII.
pub const BODY: &str = "{\"signature\":\"zz9\", \"a\": 1.50,\"t\":\"caf\u{e9}\"}";

fn picture(shade: u8) -> Picture {
    let rgba = (0..6u8).flat_map(|at| [shade, at, 0, 255]).collect();
    Picture::new(CanonicalPixels::from_rgba(3, 2, rgba).unwrap()).unwrap()
}

fn call(id: &str, name: &str) -> Part {
    let arguments = json!({"image": "a", "left": 0.25, "n": 1e3});
    Part::ToolCall(ToolCall::new(id, name, arguments).unwrap())
}

fn opaque() -> Part {
    Part::Opaque(Opaque::new("fake", BODY).unwrap())
}

/// A turn of two calls with an opaque part between them, and its usage.
fn calling() -> Turn {
    let parts = vec![
        Part::Text("I will look.".to_owned()),
        call("call-1", "view"),
        opaque(),
        call("call-2", "view"),
    ];
    let usage = Usage::new(120, 30).cached(7, 9);
    Turn::new(parts, StopReason::ToolUse, Some(usage), Some("req-1")).unwrap()
}

/// A turn that was cut off, holding what came before the cut.
fn cut_off() -> Turn {
    let parts = vec![Part::Text("The crop shows".to_owned())];
    Turn::new(parts, StopReason::MaxTokens, None, None).unwrap()
}

/// A request whose conversation holds every kind of part.
fn request() -> JudgeRequest {
    let result = ToolResult::new("call-1", "one crop", false, vec![picture(9)]).unwrap();
    let refused = ToolResult::new("call-2", "no such image", true, Vec::new()).unwrap();
    let messages = vec![
        Message::new(Role::System, vec![Part::Text("Be exact.".to_owned())]).unwrap(),
        Message::new(
            Role::User,
            vec![Part::Text("Look.".to_owned()), Part::Image(picture(1))],
        )
        .unwrap(),
        calling().message().unwrap(),
        Message::new(
            Role::User,
            vec![Part::ToolResult(result), Part::ToolResult(refused)],
        )
        .unwrap(),
    ];
    let schema = json!({"type": "object", "properties": {"left": {"type": "number"}}});
    let tools = vec![ToolDefinition::new("view", "Shows a region closer.", schema).unwrap()];
    JudgeRequest::new("You judge.", messages, tools, Controls::new(512).unwrap()).unwrap()
}

fn pictures(digest: PixelDigest) -> Option<Picture> {
    [picture(1), picture(9)]
        .into_iter()
        .find(|picture| picture.digest() == digest)
}

fn no_pictures(_: PixelDigest) -> Option<Picture> {
    None
}

#[test]
fn an_exchange_is_written_and_read_back_as_the_request_and_the_turn_it_was() {
    // Every kind of part, an opaque part, two calls in one turn, and a cut-off turn as
    // the answer: what is read back equals what was written, value for value, and the
    // opaque body is the bytes that went in.
    for answer in [Ok(cut_off()), Ok(calling())] {
        let written = write_exchange(3, &request(), &answer, Keep::Whole);
        let read = read_exchange(&written, &pictures).unwrap();
        assert_eq!(read.place, 3);
        assert_eq!(read.request, request());
        assert_eq!(read.answer, answer);
        // Written again, it is the same bytes.
        let again = write_exchange(3, &read.request, &read.answer, Keep::Whole);
        assert_eq!(again, written);
    }
    let written = write_exchange(0, &request(), &Ok(calling()), Keep::Whole);
    let read = read_exchange(&written, &pictures).unwrap();
    let turn = read.answer.unwrap();
    let Part::Opaque(part) = &turn.parts()[2] else {
        panic!("the third part is the opaque one");
    };
    assert_eq!(part.body().as_bytes(), BODY.as_bytes());
    assert_eq!(part.adapter(), "fake");
    // A fault as the answer, with its wait.
    let fault = Fault::new(FaultKind::Unavailable, "overloaded").retry_after_ms(1500);
    let written = write_exchange(1, &request(), &Err(fault.clone()), Keep::Whole);
    assert_eq!(
        read_exchange(&written, &pictures).unwrap().answer,
        Err(fault)
    );
}

#[test]
fn an_image_is_written_as_its_digest_and_read_back_from_the_picture_the_caller_gives() {
    let written = write_exchange(0, &request(), &Ok(cut_off()), Keep::Whole);
    let document: Json = serde_json::from_slice(&written).unwrap();
    let image = &document["messages"][1]["parts"][1]["image"];
    assert_eq!(image["pixels"], json!(picture(1).digest().to_string()));
    assert_eq!(
        (image["width"].as_u64(), image["height"].as_u64()),
        (Some(3), Some(2))
    );
    // No pixel is in the document: only the two digests, each once.
    assert!(written.len() < 2000, "{} bytes", written.len());
    // A caller that has no picture for a digest, or one of another size, reads nothing.
    let error = read_exchange(&written, &no_pictures).unwrap_err();
    assert_eq!(
        error,
        Unread::NoPicture {
            pixels: picture(1).digest()
        }
    );
    let mut resized = document.clone();
    resized["messages"][1]["parts"][1]["image"]["width"] = json!(4);
    let resized = serde_json::to_vec(&resized).unwrap();
    assert!(matches!(
        read_exchange(&resized, &pictures),
        Err(Unread::NoPicture { .. })
    ));
}

#[test]
fn an_opaque_part_written_as_a_digest_holds_no_body_and_is_not_read_back() {
    let written = write_exchange(0, &request(), &Ok(calling()), Keep::Digests);
    let text = String::from_utf8(written.clone()).unwrap();
    assert!(
        !text.contains("signature") && !text.contains("zz9"),
        "{text}"
    );
    let document: Json = serde_json::from_slice(&written).unwrap();
    let part = &document["answer"]["turn"]["parts"][2]["opaque_withheld"];
    assert_eq!(part["adapter"], "fake");
    assert_eq!(part["bytes"], json!(BODY.len()));
    assert_eq!(
        part["digest"],
        json!(Digest::of_bytes(BODY.as_bytes()).to_string())
    );
    // The same part in the conversation that was sent.
    let sent = &document["messages"][2]["parts"][2]["opaque_withheld"];
    assert_eq!(sent, part);
    assert_eq!(read_exchange(&written, &pictures), Err(Unread::Withheld));
}

#[test]
fn a_script_is_read_through_the_constructors_that_check_a_turn_and_a_fault() {
    let fault = Fault::new(FaultKind::Rejected, "no");
    let entries = vec![Ok(calling()), Ok(cut_off()), Err(fault)];
    let written = write_script(&entries);
    assert_eq!(read_script(&written).unwrap(), entries);
    // Written by hand, with a stop reason in a provider's word and no usage.
    let by_hand = json!({"script": SCRIPT, "entries": [
        {"turn": {"parts": [{"text": "hm"}], "stop": {"other": "pause turn"},
                  "usage": null, "request_id": "  r-9 "}},
        {"fault": {"kind": "timed_out", "sentence": "late <br>", "retry_after_ms": null}},
    ]});
    let read = read_script(&serde_json::to_vec(&by_hand).unwrap()).unwrap();
    let turn = read[0].as_ref().unwrap();
    // What a turn's constructor cleans is cleaned: the id is trimmed.
    assert_eq!(turn.request_id(), Some("r-9"));
    assert_eq!(turn.stop(), &StopReason::Other("pause turn".to_owned()));
    let fault = read[1].as_ref().unwrap_err();
    assert_eq!(fault.kind(), FaultKind::TimedOut);
    // And what a fault's constructor cleans: the marks it does not keep.
    assert_eq!(fault.sentence(), "late ?br?");
}

/// The script of one turn, with one change made to its JSON.
fn script_with(change: impl FnOnce(&mut Json)) -> Vec<u8> {
    let mut document: Json = serde_json::from_slice(&write_script(&[Ok(calling())])).unwrap();
    change(&mut document);
    serde_json::to_vec(&document).unwrap()
}

#[test]
fn data_that_breaks_a_bound_of_a_turn_is_refused_when_it_is_read() {
    let unfit = |change: &dyn Fn(&mut Json)| match read_script(&script_with(change)) {
        Err(Unread::Unfit { at: 0, why }) => why,
        other => panic!("not refused by a constructor: {other:?}"),
    };
    let parts = |d: &mut Json| d["entries"][0]["turn"]["parts"].take();
    // A text one byte over the limit.
    let long = "a".repeat(TEXT_LIMIT + 1);
    let why = unfit(&|d| d["entries"][0]["turn"]["parts"][0] = json!({"text": long}));
    assert!(matches!(why, Unfit::TooLarge { .. }), "{why}");
    // An opaque body one byte over the limit: refused, never cut.
    let why = unfit(&|d| d["entries"][0]["turn"]["parts"][2]["opaque"]["body"] = json!(long));
    assert!(matches!(why, Unfit::TooLarge { .. }), "{why}");
    // More parts than a turn holds.
    let why = unfit(&|d| {
        let many = vec![json!({"text": "x"}); 257];
        d["entries"][0]["turn"]["parts"] = json!(many);
    });
    assert!(matches!(why, Unfit::TooLarge { .. }), "{why}");
    // Two calls with one id; a call id with a space; an adapter that is no name.
    let why = unfit(&|d| d["entries"][0]["turn"]["parts"][3]["tool_call"]["id"] = json!("call-1"));
    assert_eq!(why.to_string(), "a turn holds two tool calls with one id");
    let why = unfit(&|d| d["entries"][0]["turn"]["parts"][1]["tool_call"]["id"] = json!("a b"));
    assert!(matches!(why, Unfit::NotAllowed { .. }), "{why}");
    let why = unfit(&|d| d["entries"][0]["turn"]["parts"][2]["opaque"]["adapter"] = json!("A b"));
    assert!(matches!(why, Unfit::NotAllowed { .. }), "{why}");
    // A turn that stopped for a tool and calls none; a turn that ended with nothing.
    let why = unfit(&|d| d["entries"][0]["turn"]["parts"] = json!([{"text": "x"}]));
    assert_eq!(
        why.to_string(),
        "a turn stopped to use a tool and calls none"
    );
    let why = unfit(&|d| {
        parts(d);
        d["entries"][0]["turn"]["parts"] = json!([]);
        d["entries"][0]["turn"]["stop"] = json!("end_turn");
    });
    assert_eq!(why, Unfit::Empty { what: "a turn" });
    // What only a user says: a tool result for no call, and an image.
    for part in [
        json!({"tool_result": {"call": "nobody", "text": "x", "error": false, "images": []}}),
        json!({"image": {"width": 3, "height": 2, "pixels": picture(1).digest().to_string()}}),
    ] {
        let why = unfit(&|d| d["entries"][0]["turn"]["parts"][0] = part.clone());
        assert_eq!(why.to_string(), "a turn holds what only a user says");
    }
}

#[test]
fn a_script_is_read_strictly_and_within_its_bounds() {
    let malformed = |change: &dyn Fn(&mut Json)| {
        let error = read_script(&script_with(change)).unwrap_err();
        assert!(matches!(error, Unread::Malformed(_)), "{error}");
    };
    // A field that is not known, at each level; a field left out, which may be null
    // and may not be absent; a part of no kind, and of two.
    malformed(&|d| d["extra"] = json!(1));
    malformed(&|d| d["entries"][0]["turn"]["extra"] = json!(1));
    malformed(&|d| d["entries"][0]["turn"]["parts"][1]["tool_call"]["extra"] = json!(1));
    malformed(&|d| d["entries"][0]["turn"]["usage"]["extra"] = json!(1));
    malformed(&|d| {
        drop(
            d["entries"][0]["turn"]
                .as_object_mut()
                .unwrap()
                .remove("usage"),
        )
    });
    malformed(&|d| {
        drop(
            d["entries"][0]["turn"]
                .as_object_mut()
                .unwrap()
                .remove("request_id"),
        )
    });
    malformed(&|d| d["entries"][0]["turn"]["parts"][0] = json!({"sound": "x"}));
    malformed(&|d| d["entries"][0]["turn"]["parts"][0] = json!({"text": "x", "opaque": {}}));
    malformed(&|d| d["entries"][0]["turn"]["stop"] = json!("finished"));
    malformed(&|d| d["entries"][0] = json!({"turn": {}, "fault": {}}));
    // Something after the document, and bytes that are no JSON.
    let mut trailing = write_script(&[Ok(calling())]);
    trailing.extend_from_slice(b"{}");
    assert!(matches!(read_script(&trailing), Err(Unread::Malformed(_))));
    assert!(matches!(
        read_script(b"\xff\xfe"),
        Err(Unread::Malformed(_))
    ));
    // Another version, another kind, and no kind at all: said as such, and cut.
    let other = |kind: Json| match read_script(&script_with(|d| d["script"] = kind)) {
        Err(Unread::OtherVersion { found }) => found,
        other => panic!("{other:?}"),
    };
    assert_eq!(other(json!("judge-script/2")), "judge-script/2");
    assert_eq!(other(json!(EXCHANGE)), EXCHANGE);
    assert_eq!(other(json!(7)), "none");
    assert_eq!(other(json!("v".repeat(300))).len(), 64);
    // A fault of a kind there is not.
    let fault = json!({"fault": {"kind": "tired", "sentence": "x", "retry_after_ms": null}});
    let error = read_script(&script_with(|d| d["entries"][0] = fault)).unwrap_err();
    assert_eq!(
        error,
        Unread::Unknown {
            what: "a fault's kind"
        }
    );
    // More entries than a run sends requests; more bytes than are read, which are
    // not parsed: they are no JSON at all.
    let entry = json!({"fault": {"kind": "rejected", "sentence": "x", "retry_after_ms": null}});
    let many = vec![entry; SCRIPT_ENTRIES_LIMIT + 1];
    let error = read_script(&script_with(|d| d["entries"] = json!(many))).unwrap_err();
    assert_eq!(
        error,
        Unread::TooMany {
            entries: 1025,
            limit: 1024
        }
    );
    let huge = vec![b'x'; SCRIPT_MAX_BYTES + 1];
    assert!(matches!(read_script(&huge), Err(Unread::TooLarge { .. })));
    // No refusal is as long as what it refuses, or shows a control character.
    let noisy = format!("{}\u{1b}[31m", "q".repeat(5000));
    let error = read_script(&script_with(|d| d[noisy.as_str()] = json!(1))).unwrap_err();
    let said = error.to_string();
    assert!(said.len() < 400 && !said.contains('\u{1b}'), "{said}");
}

#[test]
fn an_exchange_is_read_strictly_and_its_request_is_held_to_a_requests_bounds() {
    let with = |change: &dyn Fn(&mut Json)| {
        let written = write_exchange(0, &request(), &Ok(cut_off()), Keep::Whole);
        let mut document: Json = serde_json::from_slice(&written).unwrap();
        change(&mut document);
        read_exchange(&serde_json::to_vec(&document).unwrap(), &pictures)
    };
    assert!(matches!(
        with(&|d| d["extra"] = json!(1)),
        Err(Unread::Malformed(_))
    ));
    assert!(matches!(
        with(&|d| d["messages"][0]["role"] = json!("tool")),
        Err(Unread::Malformed(_))
    ));
    assert!(matches!(
        with(&|d| d["exchange"] = json!(SCRIPT)),
        Err(Unread::OtherVersion { .. })
    ));
    // A message its role cannot say, placed by its number; no messages; no output
    // tokens; two tools of one name; a tool whose schema is no object.
    let unfit = |change: &dyn Fn(&mut Json)| match with(change) {
        Err(Unread::Unfit { at, why }) => (at, why.to_string()),
        other => panic!("not refused by a constructor: {other:?}"),
    };
    let said = unfit(&|d| d["messages"][0]["parts"] = d["messages"][2]["parts"].clone());
    assert_eq!(said, (0, "a system message holds text only".to_owned()));
    assert_eq!(
        unfit(&|d| d["messages"] = json!([])).1,
        "the conversation is empty"
    );
    assert_eq!(unfit(&|d| d["max_output_tokens"] = json!(0)).0, 4);
    let said = unfit(&|d| d["tools"] = json!([d["tools"][0].clone(), d["tools"][0].clone()]));
    assert_eq!(said.1, "the tools define one name twice");
    assert_eq!(
        unfit(&|d| d["tools"][0]["schema"] = json!([])).1,
        "a tool's schema is not a JSON object"
    );
}

#[test]
fn the_kind_of_a_document_is_read_first_and_by_itself() {
    // Another kind is said to be one whatever the rest of the document holds, a kind
    // that is no text is none, and a kind named twice is no document.
    let other = |text: &str| match read_script(text.as_bytes()) {
        Err(Unread::OtherVersion { found }) => found,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        other(r#"{"entries":"not a list","script":"judge-script/9","x":[1,{"y":2}]}"#),
        "judge-script/9"
    );
    assert_eq!(
        other(r#"{"script":["judge-script/1"],"entries":[]}"#),
        "none"
    );
    assert_eq!(other(r#"{"entries":[]}"#), "none");
    let twice = r#"{"script":"judge-script/9","script":"judge-script/1","entries":[]}"#;
    assert!(matches!(
        read_script(twice.as_bytes()),
        Err(Unread::Malformed(_))
    ));
    for no_object in ["[]", "7", "", "{\"script\":\"judge-script/1\""] {
        let error = read_script(no_object.as_bytes()).unwrap_err();
        assert!(
            matches!(error, Unread::Malformed(_)),
            "{no_object}: {error}"
        );
    }
    // Arrays nested a hundred thousand deep are refused, and nothing overflows.
    let deep = format!(
        r#"{{"script":"judge-script/1","entries":{}{}}}"#,
        "[".repeat(100_000),
        "]".repeat(100_000)
    );
    assert!(matches!(
        read_script(deep.as_bytes()),
        Err(Unread::Malformed(_))
    ));
}

#[test]
fn an_exchange_whose_turn_was_not_written_is_not_read_back() {
    let written = write_exchange(0, &request(), &Ok(cut_off()), Keep::Whole);
    let mut document: Json = serde_json::from_slice(&written).unwrap();
    document["answer"] = json!({"turn_withheld": {
        "parts": 1, "bytes": 14, "stop": "max_tokens",
        "digest": Digest::of_bytes(b"The crop shows").to_string(),
    }});
    let bytes = serde_json::to_vec(&document).unwrap();
    assert_eq!(read_exchange(&bytes, &pictures), Err(Unread::Withheld));
}
