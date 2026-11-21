//! The tests of the OpenAI judge (plan m2c, task P1), each against a server on loopback
//! with a certificate of a root made in the test (`crate::service::testing`). Nothing
//! here contacts a host off the machine. The gate and the executor's credential are the
//! binary's wiring's, and are tested there (`crates/whizbang/tests/it/openai.rs`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod conformance;

use whizbang_core::capability::Declaration;
use whizbang_core::contract::{ModelIdentity, Secrets};
use whizbang_core::egress::Address;

use super::{ADAPTER, LONG_EDGE, VERSION, spend, wire};
use crate::service::testing::faults_hold_nothing;
use crate::service::{Service, Wire};

/// A backend of the provider at a port of loopback, for this model and reasoning
/// effort, with ceilings that hold nothing back.
fn responses(port: u16, model: &str, effort: Option<&str>) -> Service {
    let identity = ModelIdentity::new(ADAPTER, model, VERSION);
    Service {
        declaration: Declaration::new(identity, &["image", "tool"]).unwrap(),
        address: Address::of(
            &format!("https://127.0.0.1:{port}/v1/responses"),
            &Secrets::none(),
        )
        .unwrap(),
        wire: Wire::Responses(wire::Settings {
            model: model.to_owned(),
            effort: effort.map(str::to_owned),
            long_edge: LONG_EDGE,
            max_output_tokens: 128_000,
        }),
        ceilings: crate::service::spend::Ceilings {
            input_tokens: u64::MAX,
            output_tokens: 128_000,
            pictures: 64,
        },
        prices: spend::prices_of("gpt-6.1-terra").unwrap(),
    }
}

#[test]
fn a_fault_holds_nothing_of_the_body_the_headers_or_the_address() {
    faults_hold_nothing(
        |port| responses(port, "gpt-6.1-terra", None),
        wire::NOT_A_RESPONSE,
    );
}

#[test]
fn a_key_written_twice_in_a_calls_arguments_is_kept_beside_the_call() {
    // Plan e1, task F1: the arguments arrive as text, and a key written twice is lost
    // once they are parsed; where it was is kept beside the call.
    let arguments =
        r#"{"items": {"chk_ob_SF1_candidate": {"a": 1}, "chk_ob_SF1_candidate": {"a": 2}}}"#;
    let body = serde_json::json!({
        "id": "resp_1", "status": "completed",
        "output": [{ "type": "function_call", "id": "fc_1", "call_id": "call_1",
                     "name": "submit_verdict", "arguments": arguments, "status": "completed" }],
        "usage": { "input_tokens": 10, "output_tokens": 5 }
    });
    let turn = wire::turn(body.to_string().as_bytes()).unwrap();
    let call = turn.tool_calls().next().unwrap();
    assert_eq!(call.repeated_keys(), ["/items/chk_ob_SF1_candidate"]);
    assert_eq!(call.arguments()["items"]["chk_ob_SF1_candidate"]["a"], 2);
    // Arguments with no key written twice keep none.
    let once = body
        .to_string()
        .replace(r#", \"chk_ob_SF1_candidate\": {\"a\": 2}"#, "");
    let turn = wire::turn(once.as_bytes()).unwrap();
    assert!(turn.tool_calls().next().unwrap().repeated_keys().is_empty());
}
