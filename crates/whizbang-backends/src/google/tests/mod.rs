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
