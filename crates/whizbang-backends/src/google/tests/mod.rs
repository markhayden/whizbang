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
