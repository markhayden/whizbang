//! `google::…`: the Google judge through the binary's wiring (plan m2c, task P1), as
//! `anthropic::…` does for Anthropic's: a config that names the kind `service` with the
//! provider `google`, its key in the env file, and a server on loopback with a
//! certificate of a root made in the test. The shipped kind's address is never asked.

use whizbang_backends::google::KEY_HEADER;

use crate::anthropic::{GOOGLE, asked_once};

#[test]
fn the_key_goes_only_to_the_approved_origin_in_its_header() {
    let (judged, head, _) = asked_once(GOOGLE, "google-key");
    // In its one header, `x-goog-api-key`, and in no query of the address.
    let line = format!("{KEY_HEADER}: {}\r\n", judged.key);
    assert!(head.contains(&line), "{head}");
    assert!(
        head.starts_with("POST /v1beta/models/gemini-3.8-flash:generateContent HTTP/1.1\r\n"),
        "{head}"
    );
    assert!(
        !head.to_ascii_lowercase().contains("authorization"),
        "{head}"
    );
}
