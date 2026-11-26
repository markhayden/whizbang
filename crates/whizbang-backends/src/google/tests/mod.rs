//! The tests of the Google judge (plan m2c, task P1), each against a server on loopback
//! with a certificate of a root made in the test (`crate::service::testing`). Nothing
//! here contacts a host off the machine. The gate and the executor's credential are the
//! binary's wiring's, and are tested there (`crates/whizbang/tests/it/google.rs`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod conformance;

use whizbang_core::capability::Declaration;
use whizbang_core::contract::{ModelIdentity, Secrets};
use whizbang_core::egress::Address;

use super::{ADAPTER, LONG_EDGE, VERSION, spend, wire};
use crate::service::testing::faults_hold_nothing;
use crate::service::{Service, Wire};

/// A backend of the provider at a port of loopback, for this model and thinking level,
/// with ceilings that hold nothing back.
fn generate(port: u16, model: &str, effort: Option<&str>) -> Service {
    let identity = ModelIdentity::new(ADAPTER, model, VERSION);
    Service {
        declaration: Declaration::new(identity, &["image", "tool"]).unwrap(),
        address: Address::of(
            &format!("https://127.0.0.1:{port}/v1beta/models/{model}:generateContent"),
            &Secrets::none(),
        )
        .unwrap(),
        wire: Wire::Generate(wire::Settings {
            effort: effort.map(str::to_owned),
            long_edge: LONG_EDGE,
            max_output_tokens: 65_536,
        }),
        ceilings: crate::service::spend::Ceilings {
            input_tokens: u64::MAX,
            output_tokens: 65_536,
            pictures: 64,
        },
        prices: spend::prices_of("gemini-3.8-flash").unwrap(),
    }
}

#[test]
fn a_fault_holds_nothing_of_the_body_the_headers_or_the_address() {
    faults_hold_nothing(
        |port| generate(port, "gemini-3.8-flash", None),
        wire::NOT_A_RESPONSE,
    );
}

#[test]
fn a_call_the_model_could_not_form_is_a_fault_sent_again_and_charged_its_usage() {
    // The gate run of the details first ended two turns of the Google judge at the
    // finish reasons MALFORMED_FUNCTION_CALL and MALFORMED_RESPONSE, which the loop does
    // not know. Each is the model failing to form its answer, which the same request may
    // not fail again: a fault `unavailable`, which `check` sends again, answered and
    // charged its usage. The bodies have the shape of those two answers: a part that
    // holds a call, no output, and the prompt read from the cache.
    use std::sync::Arc;
    use whizbang_core::capability::{FaultKind, Stage};

    use crate::net::testing::Pki;
    use crate::service::testing::{answer, ask, serving, words};
    let pki = Arc::new(Pki::loopback());
    let body = |reason: &str| {
        serde_json::json!({
            "candidates": [{ "content": { "role": "model", "parts": [
                { "functionCall": { "id": "call_1", "name": "submit_verdict", "args": {} },
                  "thoughtSignature": "c2lnbmVk" }
            ] }, "finishReason": reason }],
            "usageMetadata": { "promptTokenCount": 10_000, "cachedContentTokenCount": 8_000,
                               "candidatesTokenCount": 0 },
            "responseId": "resp_1"
        })
        .to_string()
    };
    let reasons = ["MALFORMED_FUNCTION_CALL", "MALFORMED_RESPONSE"];
    let answers = reasons
        .iter()
        .map(|reason| answer("200 OK", "", body(reason).as_bytes()))
        .collect();
    let (listener, _) = serving(&pki, answers);
    for reason in reasons {
        let fault = ask(
            &pki,
            generate(listener.port(), "gemini-3.8-flash", None),
            &words(),
        )
        .unwrap_err();
        assert_eq!(
            (fault.kind(), fault.stage(), fault.sentence()),
            (FaultKind::Unavailable, Stage::Answered, wire::MALFORMED),
            "{reason}"
        );
        // 2,000 at 0.75 and 8,000 at 0.075, in millionths of a dollar.
        assert_eq!(fault.charge().map(|charge| charge.micros()), Some(2_100));
    }
    assert_eq!(listener.finish(), reasons.len());
}

#[test]
fn an_error_google_states_is_named_and_a_transient_one_is_sent_again_after_its_stated_wait() {
    // The gate run of the details first on 2026-11-19 ended 86 runs of the Google judge,
    // from one point of the run to its end, at `malformed: the provider answered with a
    // status the adapter does not act on`: a status outside those the kind classes, which
    // the run kept no trace of. Each answer here has the shape of Google's error (the
    // body `{"error": {"code", "message", "status", "details"}}`, its message redacted):
    // the status the adapter does not act on is named, with Google's own word for it
    // when that is one of its codes; a status Google states is transient is the fault
    // `unavailable`, which `check` sends again, after the wait Google states in a
    // `RetryInfo`; and the statuses of the kind that are busy name which they were.
    use std::sync::Arc;
    use whizbang_core::capability::{FaultKind, Stage};

    use crate::net::testing::Pki;
    use crate::service::testing::{answer, ask, serving, words};
    let pki = Arc::new(Pki::loopback());
    let error = |code: u16, word: &str, wait: Option<&str>| {
        let mut details = vec![serde_json::json!({
            "@type": "type.googleapis.com/google.rpc.ErrorInfo",
            "reason": "REDACTED", "domain": "googleapis.com"})];
        if let Some(wait) = wait {
            details.push(serde_json::json!({
                "@type": "type.googleapis.com/google.rpc.RetryInfo", "retryDelay": wait}));
        }
        serde_json::json!({"error": {"code": code, "message": "[redacted]",
                                     "status": word, "details": details}})
        .to_string()
    };
    let rate = crate::service::RATE_LIMITED;
    let busy = crate::service::BUSY;
    let status = crate::service::STATUS;
    let cases: Vec<(&str, String, FaultKind, String, Option<u64>)> = vec![
        (
            "429 Too Many Requests",
            error(429, "RESOURCE_EXHAUSTED", Some("7s")),
            FaultKind::Unavailable,
            rate.to_owned(),
            Some(7_000),
        ),
        (
            "503 Service Unavailable",
            error(503, "UNAVAILABLE", Some("1.5s")),
            FaultKind::Unavailable,
            format!("{busy}; it answered 503 UNAVAILABLE"),
            Some(1_500),
        ),
        (
            "500 Internal Server Error",
            error(500, "INTERNAL", None),
            FaultKind::Unavailable,
            format!("{busy}; it answered 500 INTERNAL"),
            None,
        ),
        (
            "408 Request Timeout",
            error(408, "DEADLINE_EXCEEDED", None),
            FaultKind::Unavailable,
            format!("{busy}; it answered 408 DEADLINE_EXCEEDED"),
            None,
        ),
        (
            "499 Client Closed Request",
            error(499, "CANCELLED", None),
            FaultKind::Malformed,
            format!("{status}; it answered 499 CANCELLED"),
            None,
        ),
        (
            "418 Teapot",
            error(418, "NOT-ONE-OF-GOOGLES", Some("9s")),
            FaultKind::Malformed,
            format!("{status}; it answered 418"),
            None,
        ),
    ];
    let answers = cases
        .iter()
        .map(|(line, body, ..)| answer(line, "", body.as_bytes()))
        .collect();
    let (listener, _) = serving(&pki, answers);
    for (line, _, kind, sentence, wait) in &cases {
        let fault = ask(
            &pki,
            generate(listener.port(), "gemini-3.8-flash", None),
            &words(),
        )
        .unwrap_err();
        assert_eq!(
            (
                fault.kind(),
                fault.stage(),
                fault.sentence(),
                fault.wait_ms()
            ),
            (*kind, Stage::Answered, sentence.as_str(), *wait),
            "{line}"
        );
        assert!(!fault.sentence().contains("redacted"), "{line}");
    }
    assert_eq!(listener.finish(), cases.len());
}

#[test]
fn submission_v3_is_sent_with_its_definitions_written_in_place() {
    // Plan mE, task F1, item 4: the conformance fixtures of v3 for this adapter. Its
    // definitions are written in place, inside the location's `anyOf` too; the items' ids
    // stay an enum of the checklist's, and an item of several instances keeps the enum of
    // the instances a finding may list.
    use crate::service::testing::{V3_FIXTURES, body_of, v3_request};
    let backend = generate(1, "gemini-3.8-flash", None);
    let Wire::Generate(settings) = &backend.wire else {
        unreachable!()
    };
    for name in V3_FIXTURES {
        let (request, schema) = v3_request(name);
        let sent = body_of(&wire::request(&request, settings).unwrap().pieces);
        let declared = sent["tools"][0]["functionDeclarations"].as_array().unwrap();
        let submit = declared
            .iter()
            .find(|d| d["name"] == "submit_verdict")
            .unwrap();
        let sent = &submit["parametersJsonSchema"];
        let text = sent.to_string();
        assert!(
            !text.contains("$ref") && !text.contains("$defs"),
            "{name}: {text}"
        );
        assert_eq!(sent["required"], schema["required"], "{name}");
        let defs = &schema["$defs"];
        let item = &sent["properties"]["items"]["items"];
        assert_eq!(
            item["properties"]["id"]["enum"], defs["Item"]["properties"]["id"]["enum"],
            "{name}"
        );
        // The item's location is `anyOf` a location or null, the location written in it.
        let location = &item["properties"]["location"]["anyOf"][0];
        assert_eq!(location["required"], defs["Location"]["required"], "{name}");
        assert_eq!(
            location["properties"]["kind"]["enum"], defs["LocationKind"]["enum"],
            "{name}"
        );
        let finding = &item["properties"]["findings"]["items"];
        assert_eq!(
            finding["properties"]["disposition"]["enum"], defs["ItemFindingDisposition"]["enum"],
            "{name}"
        );
        assert_eq!(
            finding["properties"]["instances"], defs["ItemFinding"]["properties"]["instances"],
            "{name}"
        );
    }
}
