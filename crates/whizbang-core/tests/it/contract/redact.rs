//! Redaction (`SPEC.md` section 3: "Secrets are redacted before anything is written").
//! The pass and its two tests of bodies are squint's
//! (`crates/squint-core/src/artifacts.rs`); the secrets here are made up, in the shape
//! the scan's own tests plant, and are put together at run time so that no file of the
//! repository holds one whole.

use whizbang_core::contract::{REDACTED, Secrets};

/// A made-up key: the shape of one, and the key of nothing.
pub fn made_up_key(tail: &str) -> String {
    format!("sk{}abcdefghij0123456789ABCDEFGHIJ{tail}", "-")
}

#[test]
fn every_secret_is_taken_out_of_a_text() {
    let (one, two) = (made_up_key("1"), made_up_key("2"));
    let secrets = Secrets::new([one.clone(), two.clone()]);
    let said = secrets.redact(&format!("auth {one} and again {two} end"));
    assert_eq!(said, "auth [redacted] and again [redacted] end");
    assert_eq!(REDACTED, "[redacted]");
    // A text that holds none is as it was.
    assert_eq!(secrets.redact("nothing here"), "nothing here");
}

#[test]
fn a_secret_is_found_in_the_form_a_json_string_gives_it() {
    // A secret with characters JSON escapes is caught where a body quotes it escaped.
    let secret = "pa\"ss\\word".to_owned();
    let secrets = Secrets::new([secret.clone()]);
    let body = serde_json::json!({ "text": format!("token {secret} here") }).to_string();
    let stored = secrets.redact(&body);
    assert!(!stored.contains("ss\\\\word"), "{stored}");
    assert!(stored.contains("[redacted]"));
    // And the body is still JSON.
    serde_json::from_str::<serde_json::Value>(&stored).unwrap();
}

#[test]
fn no_secret_and_an_empty_secret_replace_nothing() {
    assert_eq!(Secrets::none().redact("as it was"), "as it was");
    // An empty value would match between every two characters: it is left out.
    assert_eq!(
        Secrets::new([String::new()]).redact("as it was"),
        "as it was"
    );
    assert_eq!(Secrets::new([String::new()]), Secrets::none());
}

#[test]
fn a_secret_that_holds_another_is_replaced_whole() {
    let short = made_up_key("");
    let long = format!("{short}-and-more");
    for secrets in [
        Secrets::new([short.clone(), long.clone()]),
        Secrets::new([long.clone(), short.clone()]),
    ] {
        assert_eq!(secrets.redact(&format!("a {long} b")), "a [redacted] b");
    }
}

#[test]
fn bytes_are_redacted_when_they_are_text_and_kept_when_they_are_not() {
    let key = made_up_key("");
    let secrets = Secrets::new([key.clone()]);
    assert_eq!(
        secrets.redact_bytes(format!("x {key}").as_bytes()),
        b"x [redacted]"
    );
    // Not UTF-8, as an image is not: stored exactly.
    let mut binary = vec![0xff, 0xfe, 0x00];
    binary.extend_from_slice(key.as_bytes());
    assert_eq!(secrets.redact_bytes(&binary), binary);
}

#[test]
fn secrets_do_not_print_themselves() {
    let key = made_up_key("");
    let shown = format!("{:?}", Secrets::new([key.clone()]));
    assert_eq!(shown, "Secrets(1)");
    assert!(!shown.contains(&key));
}
