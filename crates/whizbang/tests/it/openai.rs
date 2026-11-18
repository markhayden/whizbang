//! `openai::…`: the OpenAI judge through the binary's wiring (plan m2c, task P1), as
//! `anthropic::…` does for Anthropic's: a config that names the kind `service` with the
//! provider `openai`, its key in the env file, and a server on loopback with a
//! certificate of a root made in the test. The shipped kind's address is never asked.

use whizbang_backends::openai::KEY_HEADER;

use crate::anthropic::{OPENAI, asked_once};

#[test]
fn the_key_goes_only_to_the_approved_origin_as_a_bearer() {
    let (judged, head, body) = asked_once(OPENAI, "openai-key");
    // In its one header, `Authorization`, as `Bearer` and the key.
    let line = format!("{KEY_HEADER}: Bearer {}\r\n", judged.key);
    assert!(head.contains(&line), "{head}");
    assert!(
        head.starts_with("POST /v1/responses HTTP/1.1\r\n"),
        "{head}"
    );
    assert!(!head.to_ascii_lowercase().contains("x-api-key"), "{head}");
    // Nothing the provider would keep: `store` is false.
    let sent: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(sent["store"], false);
}
