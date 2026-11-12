//! Redaction (`SPEC.md` section 3: "Secrets are redacted before anything is written";
//! ADR 0005, "Redaction"). The pass began as squint's
//! (`crates/squint-core/src/artifacts.rs`), and its two tests of bodies are the first two
//! here. The secrets are made up, in the shape the scan's own tests plant, and are put
//! together at run time so that no file of the repository holds one whole.

use whizbang_core::contract::{REDACTED, SECRET_MIN_BYTES, SecretRefused, Secrets};

/// A made-up key: the shape of one, and the key of nothing.
pub fn made_up_key(tail: &str) -> String {
    format!("sk{}abcdefghij0123456789ABCDEFGHIJ{tail}", "-")
}

fn secrets(values: &[&str]) -> Secrets {
    Secrets::new(values.iter().map(|value| (*value).to_owned())).unwrap()
}

fn holds(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[test]
fn every_secret_is_taken_out_of_a_text() {
    let (one, two) = (made_up_key("1"), made_up_key("2"));
    let secrets = secrets(&[&one, &two]);
    let said = secrets.redact(&format!("auth {one} and again {two} end"));
    assert_eq!(said, "auth [redacted] and again [redacted] end");
    assert_eq!(REDACTED, "[redacted]");
    // A text that holds none is as it was.
    assert_eq!(secrets.redact("nothing here"), "nothing here");
    // One secret twice, and three times running, is replaced each time.
    assert_eq!(
        secrets.redact(&format!("{one} then {one}")),
        "[redacted] then [redacted]"
    );
    assert_eq!(
        secrets.redact(&format!("{one}{one}{one}")),
        "[redacted][redacted][redacted]"
    );
}

#[test]
fn a_secret_is_found_in_the_form_a_json_string_gives_it() {
    // A secret with characters JSON escapes is caught where a body quotes it escaped.
    let secret = "pa\"ss\\word".to_owned();
    let secrets = secrets(&[&secret]);
    let body = serde_json::json!({ "text": format!("token {secret} here") }).to_string();
    let stored = secrets.redact(&body);
    assert!(!stored.contains("ss\\\\word"), "{stored}");
    assert_eq!(stored, r#"{"text":"token [redacted] here"}"#);
}

#[test]
fn a_redacted_json_body_is_still_json() {
    // A secret that ends in a backslash: as it stands it is the start of its own escaped
    // form. Replacing it as it stands first would leave the second backslash behind, and
    // a string that never ends.
    let secret = "abcdefgh\\".to_owned();
    let secrets = secrets(&[&secret]);
    let body = serde_json::json!({ "t": secret }).to_string();
    assert_eq!(body, r#"{"t":"abcdefgh\\"}"#);
    let stored = secrets.redact(&body);
    assert_eq!(stored, r#"{"t":"[redacted]"}"#);
    serde_json::from_str::<serde_json::Value>(&stored).unwrap();
    // As a map's key, and with a slash that the writer chose to escape.
    let slashed = "abc/defgh/ijk".to_owned();
    let secrets = Secrets::new([slashed.clone(), secret.clone()]).unwrap();
    let body = format!(
        r#"{{"abc\/defgh\/ijk":1,"{}":"abc/defgh/ijk"}}"#,
        "abcdefgh\\\\"
    );
    let stored = secrets.redact(&body);
    assert_eq!(stored, r#"{"[redacted]":1,"[redacted]":"[redacted]"}"#);
    serde_json::from_str::<serde_json::Value>(&stored).unwrap();
}

#[test]
fn a_secret_is_found_percent_encoded_as_a_url_holds_it() {
    let secret = "key/with+odd=chars&more".to_owned();
    let secrets = secrets(&[&secret]);
    let upper = "GET https://h.example/v1?key=key%2Fwith%2Bodd%3Dchars%26more failed";
    assert_eq!(
        secrets.redact(upper),
        "GET https://h.example/v1?key=[redacted] failed"
    );
    let lower = "key=key%2fwith%2bodd%3dchars%26more";
    assert_eq!(secrets.redact(lower), "key=[redacted]");
}

#[test]
fn no_secret_and_an_empty_secret_replace_nothing() {
    assert_eq!(Secrets::none().redact("as it was"), "as it was");
    // An empty value is what a variable that was never set expands to: it is no secret,
    // and is left out.
    let empty = Secrets::new([String::new()]).unwrap();
    assert_eq!(empty.redact("as it was"), "as it was");
    assert_eq!(empty, Secrets::none());
    assert_eq!(Secrets::none().redact_bytes(&[0xff, 0x00]), [0xff, 0x00]);
}

#[test]
fn a_value_too_short_to_be_a_secret_is_refused_and_not_ignored() {
    // A short value would match inside ordinary words and inside JSON's own escapes: a
    // secret `n` would turn `\n` into `\[redacted]`. Whoever meant to protect it is told.
    assert_eq!(SECRET_MIN_BYTES, 8);
    for short in ["n", "abc", "1234567"] {
        let refused = Secrets::new([made_up_key(""), short.to_owned()]).unwrap_err();
        assert_eq!(refused, SecretRefused { bytes: short.len() });
        let said = refused.to_string();
        assert!(said.contains("too short"), "{said}");
    }
    Secrets::new(["12345678".to_owned()]).unwrap();
    // Counted in bytes: four letters of two bytes each are eight.
    Secrets::new(["\u{e9}\u{e9}\u{e9}\u{e9}".to_owned()]).unwrap();
}

#[test]
fn a_secret_that_holds_another_is_replaced_whole() {
    let short = made_up_key("");
    let long = format!("{short}-and-more");
    for secrets in [secrets(&[&short, &long]), secrets(&[&long, &short])] {
        assert_eq!(secrets.redact(&format!("a {long} b")), "a [redacted] b");
    }
}

#[test]
fn secrets_that_overlap_leave_no_tail_and_the_marker_is_not_rewritten() {
    // Two secrets that share a letter, met end to end: both are found over the text as
    // it was given, and the stretch they cover is replaced once.
    let secrets = secrets(&["abcdefgX", "Xhijklmn"]);
    assert_eq!(secrets.redact("<abcdefgXhijklmn>"), "<[redacted]>");
    // A secret that is part of the marker, or the marker itself: what was put in place
    // of another secret is not searched again.
    let key = made_up_key("");
    for part in ["redacted", "[redacted]"] {
        let secrets = Secrets::new([part.to_owned(), key.clone()]).unwrap();
        assert_eq!(secrets.redact(&format!("x {key} y")), "x [redacted] y");
        assert_eq!(
            secrets.redact(&format!("{key}{key}")),
            "[redacted][redacted]"
        );
    }
}

#[test]
fn bytes_are_searched_whatever_their_encoding() {
    let key = made_up_key("");
    let secrets = secrets(&[&key]);
    assert_eq!(
        secrets.redact_bytes(format!("x {key}").as_bytes()),
        b"x [redacted]"
    );
    // Not UTF-8: a body cut in the middle of a character, or one stray byte in a
    // worker's error output. The secret beside it is still taken out.
    let mut cut = vec![0xc3];
    cut.extend_from_slice(format!(" auth {key} ").as_bytes());
    cut.extend_from_slice(&[0xff, 0xfe, 0x00]);
    let stored = secrets.redact_bytes(&cut);
    assert!(!holds(&stored, key.as_bytes()));
    let mut expected = vec![0xc3];
    expected.extend_from_slice(b" auth [redacted] ");
    expected.extend_from_slice(&[0xff, 0xfe, 0x00]);
    assert_eq!(stored, expected);
    // Bytes that hold no secret are as they were.
    assert_eq!(
        secrets.redact_bytes(&[0xff, 0x00, 0xfe]),
        [0xff, 0x00, 0xfe]
    );
}

#[test]
fn what_redaction_does_not_find_is_known() {
    // ADR 0005, "What redaction does not find". Each of these gets through, and is
    // pinned so that the list in the record stays true: the key inside base64, a letter
    // of it written as a `\u` escape, and the key in UTF-16.
    let key = made_up_key("");
    let secrets = secrets(&[&key]);
    let escaped = key.replacen('a', "\\u0061", 1);
    assert_eq!(secrets.redact(&escaped), escaped);
    let wide: Vec<u8> = key.encode_utf16().flat_map(u16::to_le_bytes).collect();
    assert_eq!(secrets.redact_bytes(&wide), wide);
    // Base64 of `user:<key>`, by hand: the alphabet is the standard one.
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let plain = format!("user:{key}");
    let mut encoded = String::new();
    for chunk in plain.as_bytes().chunks(3) {
        let padded = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let bits = u32::from(padded[0]) << 16 | u32::from(padded[1]) << 8 | u32::from(padded[2]);
        for place in 0..4 {
            if place <= chunk.len() {
                encoded.push(char::from(
                    alphabet[(bits >> (18 - 6 * place) & 63) as usize],
                ));
            } else {
                encoded.push('=');
            }
        }
    }
    assert_eq!(secrets.redact(&encoded), encoded);
}

#[test]
fn many_secrets_over_a_large_body_are_one_pass() {
    // Two thousand secrets over eight mebibytes. The time is not asserted, only that it
    // is done as one pass does it: every secret found, wherever it is.
    let keys: Vec<String> = (0..2000)
        .map(|n| made_up_key(&format!("-{n:04}")))
        .collect();
    let secrets = Secrets::new(keys.clone()).unwrap();
    let mut body = vec![b'.'; 8 * 1024 * 1024];
    for (index, key) in keys.iter().enumerate() {
        let at = index * 4000;
        body[at..at + key.len()].copy_from_slice(key.as_bytes());
    }
    let stored = secrets.redact_bytes(&body);
    assert_eq!(
        stored.len(),
        body.len() - keys.iter().map(String::len).sum::<usize>() + 2000 * REDACTED.len()
    );
    assert!(!holds(&stored, keys[0].as_bytes()));
    assert!(!holds(&stored, keys[1999].as_bytes()));
}

#[test]
fn secrets_do_not_print_themselves() {
    let key = made_up_key("");
    let shown = format!("{:?}", secrets(&[&key]));
    assert_eq!(shown, "Secrets(1)");
    assert!(!shown.contains(&key));
}
