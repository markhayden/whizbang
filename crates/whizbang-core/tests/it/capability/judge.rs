//! Judge (`SPEC.md` section 4; ADR 0006, "Judge"): a conversation of ordered mixed
//! content goes out with tool definitions and controls; a turn comes back, with its
//! usage, or a fault. A part only its adapter reads is kept whole.

use std::sync::Arc;
use std::sync::Mutex;

use serde_json::json;
use whizbang_core::capability::{
    Controls, Declaration, Fault, FaultKind, Judge, JudgeHandle, JudgeRequest, MESSAGE_LIMIT,
    Message, Opaque, PART_LIMIT, Part, Role, StopReason, TEXT_LIMIT, TOOL_LIMIT, ToolCall,
    ToolDefinition, ToolResult, Turn, Unfit, Usage, feature,
};
use whizbang_core::contract::{Cancellation, ModelIdentity};

use super::fill::picture;

fn text(words: &str) -> Part {
    Part::Text(words.to_owned())
}

fn call(id: &str) -> ToolCall {
    ToolCall::new(id, "view", json!({"left": 1, "top": 2.5})).unwrap()
}

/// The body of a part of the kind a provider sends for its own later reading: bytes
/// that are not even JSON, with the spacing, the order and the ends they came with.
const BODY: &str = " \n{ \"z\": 1,\n\t\"a\": [1.50, 2e0],  \"sig\": \"\u{e9}==\" } trailing\r\n ";

fn opaque(adapter: &str) -> Opaque {
    Opaque::new(adapter, BODY).unwrap()
}

fn not_allowed(error: Unfit) -> (&'static str, &'static str) {
    match error {
        Unfit::NotAllowed { what, why } => (what, why),
        other => panic!("not refused as not allowed: {other:?}"),
    }
}

// --- parts ----------------------------------------------------------------------------------------

#[test]
fn an_opaque_part_is_kept_whole() {
    let part = opaque("stub");
    assert_eq!(part.adapter(), "stub");
    // Byte for byte: nothing is parsed, ordered, trimmed or printed again.
    assert_eq!(part.body().as_bytes(), BODY.as_bytes());
    assert!(BODY.starts_with(' ') && BODY.ends_with(' '));
    // And it is the same part after it has been through a turn and a message.
    let turn = Turn::new(
        vec![text("thinking"), Part::Opaque(part.clone()), text("done")],
        StopReason::EndTurn,
        None,
        None,
    )
    .unwrap();
    let echoed = turn.message().unwrap();
    assert_eq!(echoed.role(), Role::Assistant);
    assert_eq!(echoed.parts()[1], Part::Opaque(part));
    assert_eq!(echoed.parts(), turn.parts());

    assert_eq!(
        Opaque::new("stub", "").unwrap_err(),
        Unfit::Empty {
            what: "an opaque part"
        }
    );
    assert!(matches!(
        Opaque::new("stub", &"x".repeat(TEXT_LIMIT + 1)),
        Err(Unfit::TooLarge { .. })
    ));
    assert!(matches!(
        Opaque::new("Not A Name", "x"),
        Err(Unfit::NotAllowed { .. })
    ));
}

#[test]
fn a_tool_call_is_an_id_a_name_and_arguments() {
    let call = call("call_1");
    assert_eq!(call.id(), "call_1");
    assert_eq!(call.name(), "view");
    assert_eq!(call.arguments(), &json!({"left": 1, "top": 2.5}));
    // An id and a name are short and plain; a model may still name a tool that does not
    // exist, and the call is kept so that it can be answered.
    ToolCall::new("toolu_01A-b.c:d", "no_such_tool", json!(null)).unwrap();
    for (id, name) in [
        ("", "view"),
        ("call 1", "view"),
        ("call\n1", "view"),
        ("caf\u{e9}", "view"),
        ("call_1", ""),
        ("call_1", "vi ew"),
    ] {
        assert!(
            matches!(
                ToolCall::new(id, name, json!({})),
                Err(Unfit::NotAllowed { .. })
            ),
            "{id:?} {name:?}"
        );
    }
    assert!(ToolCall::new(&"i".repeat(129), "view", json!({})).is_err());
    assert!(ToolCall::new("call_1", &"n".repeat(65), json!({})).is_err());
    let large = json!({"text": "x".repeat(TEXT_LIMIT)});
    assert!(matches!(
        ToolCall::new("call_1", "view", large),
        Err(Unfit::TooLarge { .. })
    ));
}

#[test]
fn a_tool_result_answers_a_call_with_text_images_or_both() {
    let result = ToolResult::new("call_1", "a crop", false, vec![picture()]).unwrap();
    assert_eq!(result.call(), "call_1");
    assert_eq!(result.text(), "a crop");
    assert!(!result.is_error());
    assert_eq!(result.images(), [picture()]);
    ToolResult::new("call_1", "no such tool", true, Vec::new()).unwrap();
    ToolResult::new("call_1", "", false, vec![picture()]).unwrap();
    assert_eq!(
        ToolResult::new("call_1", "", false, Vec::new()).unwrap_err(),
        Unfit::Empty {
            what: "a tool result"
        }
    );
    assert!(ToolResult::new("", "text", false, Vec::new()).is_err());
    assert!(matches!(
        ToolResult::new("call_1", "text", false, vec![picture(); 17]),
        Err(Unfit::TooLarge { limit: 16, .. })
    ));
}

// --- messages -----------------------------------------------------------------------------------

#[test]
fn a_message_keeps_its_parts_in_order() {
    let result = ToolResult::new("call_1", "a crop", false, vec![picture()]).unwrap();
    let parts = vec![
        text("look at this"),
        Part::Image(picture()),
        text("and at this"),
        Part::ToolResult(result),
        Part::Image(picture()),
    ];
    let message = Message::new(Role::User, parts.clone()).unwrap();
    assert_eq!(message.role(), Role::User);
    assert_eq!(message.parts(), parts);
}

#[test]
fn each_role_holds_only_what_it_can_say() {
    let result = Part::ToolResult(ToolResult::new("c", "text", false, Vec::new()).unwrap());
    let kinds = [
        ("text", text("words")),
        ("image", Part::Image(picture())),
        ("call", Part::ToolCall(call("c"))),
        ("result", result),
        ("opaque", Part::Opaque(opaque("stub"))),
    ];
    let allowed = [
        (Role::System, vec!["text"], "a system message"),
        (
            Role::User,
            vec!["text", "image", "result"],
            "a user message",
        ),
        (
            Role::Assistant,
            vec!["text", "call", "opaque"],
            "an assistant message",
        ),
    ];
    for (role, may, what) in allowed {
        for (kind, part) in &kinds {
            let made = Message::new(role, vec![part.clone()]);
            if may.contains(kind) {
                made.unwrap();
            } else {
                assert_eq!(not_allowed(made.unwrap_err()).0, what, "{kind}");
            }
        }
    }
}

#[test]
fn a_message_is_never_empty_and_never_unbounded() {
    assert_eq!(
        Message::new(Role::User, Vec::new()).unwrap_err(),
        Unfit::Empty { what: "a message" }
    );
    assert_eq!(
        Message::new(Role::User, vec![text("")]).unwrap_err(),
        Unfit::Empty { what: "a text" }
    );
    Message::new(Role::User, vec![text("x"); PART_LIMIT]).unwrap();
    assert_eq!(
        Message::new(Role::User, vec![text("x"); PART_LIMIT + 1]).unwrap_err(),
        Unfit::TooLarge {
            what: "a message, in parts,",
            size: 257,
            limit: 256,
        }
    );
    Message::new(Role::User, vec![text(&"x".repeat(TEXT_LIMIT))]).unwrap();
    assert_eq!(
        Message::new(Role::User, vec![text(&"x".repeat(TEXT_LIMIT + 1))]).unwrap_err(),
        Unfit::TooLarge {
            what: "a text, in bytes,",
            size: 1_048_577,
            limit: 1_048_576,
        }
    );
}

// --- the request ----------------------------------------------------------------------------------

fn tool(name: &str) -> ToolDefinition {
    ToolDefinition::new(
        name,
        "Returns a closer look at a part of an image.",
        json!({"type": "object", "properties": {"left": {"type": "number"}}}),
    )
    .unwrap()
}

fn user(words: &str) -> Message {
    Message::new(Role::User, vec![text(words)]).unwrap()
}

fn controls() -> Controls {
    Controls::new(1024).unwrap()
}

#[test]
fn a_tool_definition_is_a_name_a_description_and_a_schema() {
    let view = tool("view");
    assert_eq!(view.name(), "view");
    assert_eq!(
        view.description(),
        "Returns a closer look at a part of an image."
    );
    assert_eq!(view.schema()["type"], "object");
    assert!(ToolDefinition::new("View", "d", json!({})).is_err());
    assert!(ToolDefinition::new("view", "", json!({})).is_err());
    // A schema is a JSON object.
    for schema in [json!(null), json!("object"), json!([1])] {
        assert_eq!(
            not_allowed(ToolDefinition::new("view", "d", schema).unwrap_err()),
            ("a tool's schema", "is not a JSON object")
        );
    }
    let large = json!({"description": "x".repeat(TEXT_LIMIT)});
    assert!(matches!(
        ToolDefinition::new("view", "d", large),
        Err(Unfit::TooLarge { .. })
    ));
}

#[test]
fn a_request_is_a_conversation_its_tools_and_its_controls() {
    let messages = vec![user("is the sky blue"), user("answer yes or no")];
    let request = JudgeRequest::new(
        "You are a judge.",
        messages.clone(),
        vec![tool("view"), tool("measure")],
        controls(),
    )
    .unwrap();
    assert_eq!(request.system(), "You are a judge.");
    assert_eq!(request.messages(), messages);
    assert_eq!(request.tools().len(), 2);
    assert_eq!(request.tools()[1].name(), "measure");
    assert_eq!(request.controls().max_output_tokens(), 1024);

    let refused = |messages: Vec<Message>, tools: Vec<ToolDefinition>| {
        JudgeRequest::new("", messages, tools, controls()).unwrap_err()
    };
    assert_eq!(
        refused(Vec::new(), Vec::new()),
        Unfit::Empty {
            what: "the conversation"
        }
    );
    JudgeRequest::new("", vec![user("x"); MESSAGE_LIMIT], Vec::new(), controls()).unwrap();
    assert!(matches!(
        refused(vec![user("x"); MESSAGE_LIMIT + 1], Vec::new()),
        Unfit::TooLarge { limit: 1024, .. }
    ));
    assert!(matches!(
        refused(messages.clone(), vec![tool("view"); TOOL_LIMIT + 1]),
        Unfit::TooLarge { limit: 128, .. }
    ));
    assert_eq!(
        not_allowed(refused(messages.clone(), vec![tool("view"), tool("view")])),
        ("the tools", "define one name twice")
    );
    assert!(matches!(
        JudgeRequest::new(
            &"s".repeat(TEXT_LIMIT + 1),
            messages,
            Vec::new(),
            controls()
        ),
        Err(Unfit::TooLarge { .. })
    ));
    assert!(matches!(Controls::new(0), Err(Unfit::OutOfRange { .. })));
}

#[test]
fn a_conversation_is_bounded_in_all_it_holds() {
    // 65 messages of a megabyte each: every one is allowed, and together they are not.
    let large = Message::new(Role::User, vec![text(&"x".repeat(TEXT_LIMIT))]).unwrap();
    JudgeRequest::new("", vec![large.clone(); 64], Vec::new(), controls()).unwrap();
    assert_eq!(
        JudgeRequest::new("", vec![large; 65], Vec::new(), controls()).unwrap_err(),
        Unfit::TooLarge {
            what: "the conversation, in bytes of text,",
            size: 65 * 1_048_576,
            limit: 64 * 1_048_576,
        }
    );
}

#[test]
fn every_kind_of_text_counts_toward_the_bound_on_a_conversation() {
    // A part of each kind that holds text, a little under a megabyte of it: 64 fit and
    // 65 do not, whichever kind it is.
    let long = "x".repeat(TEXT_LIMIT - 64);
    let result = ToolResult::new("c", &long, false, Vec::new()).unwrap();
    let call = ToolCall::new("c", "view", json!({ "text": long })).unwrap();
    let kinds = [
        (Role::User, text(&long)),
        (Role::User, Part::ToolResult(result)),
        (
            Role::Assistant,
            Part::Opaque(Opaque::new("stub", &long).unwrap()),
        ),
        (Role::Assistant, Part::ToolCall(call)),
    ];
    for (role, part) in kinds {
        let message = Message::new(role, vec![part.clone()]).unwrap();
        JudgeRequest::new("", vec![message.clone(); 64], Vec::new(), controls()).unwrap();
        assert!(
            matches!(
                JudgeRequest::new("", vec![message; 65], Vec::new(), controls()),
                Err(Unfit::TooLarge {
                    what: "the conversation, in bytes of text,",
                    ..
                })
            ),
            "{part:.40?}"
        );
    }
    // The system prompt counts too.
    let large = Message::new(Role::User, vec![text(&"x".repeat(TEXT_LIMIT))]).unwrap();
    assert!(matches!(
        JudgeRequest::new("s", vec![large; 64], Vec::new(), controls()),
        Err(Unfit::TooLarge {
            size: 67_108_865,
            ..
        })
    ));
}

// --- the turn ---------------------------------------------------------------------------------------

#[test]
fn a_turn_is_ordered_content_a_stop_reason_and_usage() {
    let usage = Usage {
        input_tokens: 1200,
        output_tokens: 80,
        cache_read_tokens: 1000,
        cache_write_tokens: 0,
    };
    let parts = vec![
        Part::Opaque(opaque("stub")),
        text("I need a closer look."),
        Part::ToolCall(call("call_1")),
        Part::ToolCall(call("call_2")),
    ];
    let turn = Turn::new(
        parts.clone(),
        StopReason::ToolUse,
        Some(usage),
        Some("req_123"),
    )
    .unwrap();
    assert_eq!(turn.parts(), parts);
    assert_eq!(turn.stop(), &StopReason::ToolUse);
    assert_eq!(turn.usage(), Some(usage));
    assert_eq!(turn.request_id(), Some("req_123"));
    // Several calls in one turn, in the order they were made.
    let ids: Vec<&str> = turn.tool_calls().map(ToolCall::id).collect();
    assert_eq!(ids, ["call_1", "call_2"]);
    assert_eq!(turn.texts().collect::<Vec<_>>(), ["I need a closer look."]);

    // A backend that reported no usage is told apart from one that reported none used.
    let silent = Turn::new(vec![text("yes")], StopReason::EndTurn, None, None).unwrap();
    assert_eq!(silent.usage(), None);
    assert_eq!(silent.request_id(), None);
    assert_ne!(Some(Usage::default()), silent.usage());
}

#[test]
fn a_turn_holds_only_what_an_assistant_says() {
    let refused =
        |parts: Vec<Part>, stop: StopReason| Turn::new(parts, stop, None, None).unwrap_err();
    let result = Part::ToolResult(ToolResult::new("c", "text", false, Vec::new()).unwrap());
    for part in [Part::Image(picture()), result] {
        assert_eq!(
            not_allowed(refused(vec![part], StopReason::EndTurn)).0,
            "a turn"
        );
    }
    assert_eq!(
        refused(vec![text("")], StopReason::EndTurn),
        Unfit::Empty { what: "a text" }
    );
    assert!(matches!(
        refused(vec![text("x"); PART_LIMIT + 1], StopReason::EndTurn),
        Unfit::TooLarge { .. }
    ));
    // Two calls with one id could not be answered apart.
    let twice = vec![Part::ToolCall(call("c")), Part::ToolCall(call("c"))];
    assert_eq!(
        not_allowed(refused(twice, StopReason::ToolUse)),
        ("a turn", "holds two tool calls with one id")
    );
    // A turn that stopped to use a tool and calls none.
    assert_eq!(
        not_allowed(refused(vec![text("hm")], StopReason::ToolUse)),
        ("a turn", "stopped to use a tool and calls none")
    );
}

#[test]
fn a_turn_with_nothing_in_it_is_refused_unless_it_was_cut_off_or_declined() {
    for stop in [StopReason::EndTurn, StopReason::ToolUse] {
        assert_eq!(
            Turn::new(Vec::new(), stop, None, None).unwrap_err(),
            Unfit::Empty { what: "a turn" }
        );
    }
    for stop in [
        StopReason::MaxTokens,
        StopReason::Refusal,
        StopReason::Other("pause".to_owned()),
    ] {
        let turn = Turn::new(Vec::new(), stop.clone(), None, None).unwrap();
        assert_eq!(turn.stop(), &stop);
        // There is nothing to say back.
        assert!(turn.message().is_none());
    }
    // A cut-off turn keeps what came before the cut.
    let cut = Turn::new(vec![text("The sky is")], StopReason::MaxTokens, None, None).unwrap();
    assert_eq!(cut.texts().collect::<Vec<_>>(), ["The sky is"]);
}

#[test]
fn what_a_backend_names_is_cleaned_before_it_is_kept() {
    // A stop reason and a request id are a provider's words, and end up in a record.
    let turn = Turn::new(
        vec![text("x")],
        StopReason::Other("weird/reason\n".to_owned() + &"r".repeat(100)),
        None,
        Some(&("req/1 <b>".to_owned() + &"i".repeat(200))),
    )
    .unwrap();
    let StopReason::Other(reason) = turn.stop() else {
        panic!("another stop reason");
    };
    assert_eq!(reason.len(), 64);
    assert!(reason.starts_with("weird?reason?rrr"));
    let id = turn.request_id().unwrap();
    assert_eq!(id.len(), 128);
    assert!(id.starts_with("req?1 ?b?iii"));
    // An empty request id is no request id.
    let turn = Turn::new(vec![text("x")], StopReason::EndTurn, None, Some("")).unwrap();
    assert_eq!(turn.request_id(), None);
}

// --- the handle -------------------------------------------------------------------------------------

/// A judge that answers with what it was built to answer and keeps what it was sent.
struct Stub {
    declaration: Declaration,
    answer: Result<Turn, Fault>,
    sent: Mutex<Vec<JudgeRequest>>,
}

impl Stub {
    fn new(features: &[&str], answer: Result<Turn, Fault>) -> Arc<Stub> {
        Arc::new(Stub {
            declaration: Declaration::new(ModelIdentity::new("stub", "judge", "1"), features)
                .unwrap(),
            answer,
            sent: Mutex::new(Vec::new()),
        })
    }

    fn sent(&self) -> usize {
        self.sent.lock().unwrap().len()
    }
}

impl Judge for Stub {
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn send(&self, request: &JudgeRequest, _: &Cancellation) -> Result<Turn, Fault> {
        self.sent.lock().unwrap().push(request.clone());
        self.answer.clone()
    }
}

fn yes() -> Turn {
    Turn::new(vec![text("yes")], StopReason::EndTurn, None, None).unwrap()
}

fn plain() -> JudgeRequest {
    JudgeRequest::new("", vec![user("is the sky blue")], Vec::new(), controls()).unwrap()
}

#[test]
fn the_handle_sends_the_conversation_as_it_is_and_hands_back_the_turn() {
    let stub = Stub::new(&[feature::IMAGE, feature::TOOL], Ok(yes()));
    let handle = JudgeHandle::new(stub.clone());
    let assistant = Message::new(
        Role::Assistant,
        vec![Part::Opaque(opaque("stub")), Part::ToolCall(call("call_1"))],
    )
    .unwrap();
    let result = ToolResult::new("call_1", "a crop", false, vec![picture()]).unwrap();
    let messages = vec![
        Message::new(Role::User, vec![text("look"), Part::Image(picture())]).unwrap(),
        assistant,
        Message::new(Role::User, vec![Part::ToolResult(result)]).unwrap(),
    ];
    let request = JudgeRequest::new("judge", messages, vec![tool("view")], controls()).unwrap();
    assert_eq!(handle.send(&request, &Cancellation::new()).unwrap(), yes());
    // What reached the backend is the request, whole and in order.
    assert_eq!(stub.sent.lock().unwrap().as_slice(), [request]);
    assert!(format!("{handle:?}").contains("judge"));
}

#[test]
fn images_and_tools_are_sent_only_to_a_judge_that_declares_them() {
    let image = Message::new(Role::User, vec![Part::Image(picture())]).unwrap();
    let result = ToolResult::new("c", "", false, vec![picture()]).unwrap();
    let in_result = Message::new(Role::User, vec![Part::ToolResult(result)]).unwrap();
    let cases = [
        (vec![image], Vec::new(), feature::IMAGE),
        (vec![in_result], Vec::new(), feature::IMAGE),
        (vec![user("x")], vec![tool("view")], feature::TOOL),
    ];
    for (messages, tools, needed) in cases {
        let other = if needed == feature::IMAGE {
            feature::TOOL
        } else {
            feature::IMAGE
        };
        let request = JudgeRequest::new("", messages, tools, controls()).unwrap();
        let stub = Stub::new(&[other], Ok(yes()));
        let fault = JudgeHandle::new(stub.clone())
            .send(&request, &Cancellation::new())
            .unwrap_err();
        assert_eq!(fault.kind(), FaultKind::Unsupported);
        assert_eq!(
            fault.sentence(),
            format!("the backend judge does not declare the feature {needed}")
        );
        assert_eq!(stub.sent(), 0);
        let stub = Stub::new(&[needed], Ok(yes()));
        JudgeHandle::new(stub.clone())
            .send(&request, &Cancellation::new())
            .unwrap();
        assert_eq!(stub.sent(), 1);
    }
    // A conversation of text alone needs neither.
    let stub = Stub::new(&[], Ok(yes()));
    JudgeHandle::new(stub.clone())
        .send(&plain(), &Cancellation::new())
        .unwrap();
    assert_eq!(stub.sent(), 1);
}

#[test]
fn an_opaque_part_goes_only_to_the_adapter_that_made_it_and_comes_only_from_it() {
    // A conversation that holds another adapter's part is not sent to this one.
    let foreign = Message::new(Role::Assistant, vec![Part::Opaque(opaque("other"))]).unwrap();
    let request = JudgeRequest::new("", vec![user("x"), foreign], Vec::new(), controls()).unwrap();
    let stub = Stub::new(&[], Ok(yes()));
    let fault = JudgeHandle::new(stub.clone())
        .send(&request, &Cancellation::new())
        .unwrap_err();
    assert_eq!(fault.kind(), FaultKind::Unsupported);
    assert_eq!(
        fault.sentence(),
        "the conversation holds a part that only the adapter other reads, and the backend is stub"
    );
    assert_eq!(stub.sent(), 0);
    // And a turn that claims another adapter's part is malformed.
    let claimed = Turn::new(
        vec![Part::Opaque(opaque("other"))],
        StopReason::EndTurn,
        None,
        None,
    )
    .unwrap();
    let stub = Stub::new(&[], Ok(claimed));
    let fault = JudgeHandle::new(stub)
        .send(&plain(), &Cancellation::new())
        .unwrap_err();
    assert_eq!(fault.kind(), FaultKind::Malformed);
    assert_eq!(
        fault.sentence(),
        "a turn holds a part of another adapter than the backend that answered"
    );
}

#[test]
fn nothing_is_sent_once_the_run_is_cancelled_and_a_fault_is_handed_back_as_it_is() {
    let stub = Stub::new(&[], Ok(yes()));
    let cancellation = Cancellation::new();
    cancellation.cancel();
    let fault = JudgeHandle::new(stub.clone())
        .send(&plain(), &cancellation)
        .unwrap_err();
    assert_eq!(fault.kind(), FaultKind::Cancelled);
    assert_eq!(stub.sent(), 0);

    let refused = Fault::new(FaultKind::Credential, "status 401");
    let stub = Stub::new(&[], Err(refused.clone()));
    assert_eq!(
        JudgeHandle::new(stub)
            .send(&plain(), &Cancellation::new())
            .unwrap_err(),
        refused
    );
}
