//! Redaction (`SPEC.md` section 3: "Secrets are redacted before anything is written";
//! ADR 0005, "Redaction"; ADR 0013, which changes what is found). The pass began as squint's
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
    // A secret with a slash and a quote, as a writer that leaves the slash alone gives
    // it, and as one that escapes the slash gives it.
    let both = "ab/cd\"efgh".to_owned();
    let secrets = Secrets::new([both.clone()]).unwrap();
    assert_eq!(serde_json::to_string(&both).unwrap(), r#""ab/cd\"efgh""#);
    assert_eq!(
        secrets.redact(r#"{"t":"ab/cd\"efgh"}"#),
        r#"{"t":"[redacted]"}"#
    );
    assert_eq!(
        secrets.redact(r#"{"t":"ab\/cd\"efgh"}"#),
        r#"{"t":"[redacted]"}"#
    );
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
fn a_secret_that_ends_or_starts_with_a_quote_keeps_its_json_form() {
    // The secret's own quote is escaped inside a JSON string, and is not the quote that
    // ends the string: exactly one quote is taken from each end of what the writer gives.
    for secret in ["abcdefgh\"", "\"abcdefgh", "\"abcdefg\"", "abcdefg\"\""] {
        let secrets = secrets(&[secret]);
        let body = serde_json::json!({ "t": secret, "u": 1 }).to_string();
        let stored = secrets.redact(&body);
        assert_eq!(stored, r#"{"t":"[redacted]","u":1}"#, "{secret}");
        serde_json::from_str::<serde_json::Value>(&stored).unwrap();
    }
}

#[test]
fn a_secret_is_trimmed_of_the_space_around_it_and_of_nothing_else() {
    // A value read from a file or a variable often carries a line break or a space at
    // an end. What is searched for is the value without them.
    let key = made_up_key("");
    let padded = Secrets::new([format!("  {key} \r\n\t")]).unwrap();
    assert_eq!(padded, secrets(&[&key]));
    assert_eq!(padded.redact(&format!("auth={key};")), "auth=[redacted];");
    // A value that is only space is left out, as an empty one is.
    assert_eq!(
        Secrets::new([" \n\t ".to_owned()]).unwrap(),
        Secrets::none()
    );
    // The least length is of the trimmed value, and the refusal counts its bytes.
    assert_eq!(
        Secrets::new(["   1234567   ".to_owned()]).unwrap_err(),
        SecretRefused { bytes: 7 }
    );
    Secrets::new([" 12345678 ".to_owned()]).unwrap();
    // Space inside the value is part of it. So are quotes around it: taking those off
    // is for whoever reads the file the value came from.
    let spaced = secrets(&["abcd efgh"]);
    assert_eq!(
        spaced.redact("x abcd efgh y abcdefgh"),
        "x [redacted] y abcdefgh"
    );
    let quoted = Secrets::new([format!("\"{key}\"")]).unwrap();
    assert_eq!(quoted.redact(&format!("a {key} b")), format!("a {key} b"));
    assert_eq!(quoted.redact(&format!("a \"{key}\" b")), "a [redacted] b");
}

#[test]
fn a_tilde_and_the_other_unreserved_marks_are_not_percent_encoded() {
    // `-`, `.`, `_` and `~` stand as they are in a URL; only what is reserved is
    // written as a percent and two hex digits.
    let secrets = secrets(&["ab~cd-ef.gh_ij/kl"]);
    assert_eq!(
        secrets.redact("https://h.example/?k=ab~cd-ef.gh_ij%2Fkl&x=1"),
        "https://h.example/?k=[redacted]&x=1"
    );
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
    } // In the middle of the longer one, not at its start: the shorter is found later
    // and ends sooner, and the stretch still runs to the longer one's end.
    let around = format!("before-{short}-and-after");
    for secrets in [secrets(&[&short, &around]), secrets(&[&around, &short])] {
        assert_eq!(secrets.redact(&format!("a {around} b")), "a [redacted] b");
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

/// Base64 of these bytes, by hand, in the standard alphabet or the one for URLs.
fn base64(plain: &[u8], url: bool) -> String {
    let alphabet: &[u8; 64] = if url {
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_"
    } else {
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
    };
    let mut encoded = String::new();
    for chunk in plain.chunks(3) {
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
    encoded
}

#[test]
fn a_secret_is_found_percent_encoded_in_any_mixture() {
    // ADR 0013: percent-encoding is read, not matched against one written form. The
    // five forms that ADR 0005 listed as getting through are found: the two cases of
    // hex mixed in one value, a reserved byte left plain beside encoded ones, a space
    // written `+`, a byte encoded that need not be, and a value encoded twice.
    let odd = "ab cd/ef+gh=ij".to_owned();
    let found = secrets(&[&odd]);
    for body in [
        "k=ab%20cd%2Fef%2Bgh%3Dij",
        "k=ab%20cd%2fef%2Bgh%3dij",
        "k=ab%20cd/ef%2Bgh%3Dij",
        "k=ab+cd%2Fef%2Bgh%3Dij",
        "k=ab+cd/ef%2bgh=ij",
        "k=%61b%20cd%2Fef%2Bgh%3Dij",
        "k=%61%62%20%63%64%2F%65%66%2B%67%68%3D%69%6A",
        "k=ab%2520cd%252Fef%252Bgh%253Dij",
        "k=ab%2520cd%252fef%2Bgh=ij",
    ] {
        assert_eq!(
            found.redact(&format!("{body}&x=1")),
            "k=[redacted]&x=1",
            "{body}"
        );
    }
    // A `+` is a space only where the rest agrees: the secret's own `+`, written
    // plain, is found as it is, and a body that only looks alike is left alone.
    assert_eq!(found.redact("k=ab cd/ef+gh=ij;"), "k=[redacted];");
    assert_eq!(found.redact("k=ab+cd/ef gh=ij;"), "k=ab+cd/ef gh=ij;");
    // Text that holds a percent sign and no secret is as it was.
    for plain in ["100% of 20%2", "a%zzb+c", "%", "%4", "50%+"] {
        assert_eq!(found.redact(plain), plain);
    }
}

#[test]
fn a_secret_is_found_behind_the_escapes_of_a_json_string() {
    let key = made_up_key("");
    let secrets = secrets(&[&key]);
    // One letter as a `\u` escape, in either case of hex; every letter as one; and
    // the hyphen as the escape a cautious writer gives it.
    let one = key.replacen('a', "\\u0061", 1);
    let upper = key.replacen('j', "\\u006A", 1);
    let lower = key.replacen('j', "\\u006a", 1);
    let all: String = key
        .chars()
        .map(|c| format!("\\u{:04x}", c as u32))
        .collect();
    for escaped in [one, upper, lower, all] {
        let body = format!("{{\"t\":\"auth {escaped} end\"}}");
        assert_eq!(
            secrets.redact(&body),
            "{\"t\":\"auth [redacted] end\"}",
            "{body}"
        );
    }
    // A letter outside ASCII, as itself, as its escape, and as the two escapes of a
    // letter outside the first plane.
    let wide = "cl\u{e9}-\u{1f511}-abcdef".to_owned();
    let secrets = Secrets::new([wide.clone()]).unwrap();
    for escaped in [
        wide.clone(),
        "cl\\u00e9-\\ud83d\\udd11-abcdef".to_owned(),
        "cl\\u00E9-\\uD83D\\uDD11-abcdef".to_owned(),
        "\\u0063l\u{e9}-\u{1f511}-abc\\u0064ef".to_owned(),
    ] {
        assert_eq!(
            secrets.redact(&format!("<{escaped}>")),
            "<[redacted]>",
            "{escaped}"
        );
    }
    // What is no escape is left as it is: half of a pair, too few digits, a letter
    // that is no hex digit.
    for plain in ["\\ud83d-abcdef", "\\u00e", "\\u00zz", "a\\qb", "\\"] {
        assert_eq!(secrets.redact(plain), plain);
    }
}

#[test]
fn a_secret_is_found_in_utf_16_of_either_byte_order() {
    let key = made_up_key("");
    let wide = "cl\u{e9}-\u{1f511}-abcdef".to_owned();
    let secrets = Secrets::new([key.clone(), wide.clone()]).unwrap();
    for secret in [&key, &wide] {
        for big in [false, true] {
            let encode = |text: &str| -> Vec<u8> {
                text.encode_utf16()
                    .flat_map(|unit| {
                        if big {
                            unit.to_be_bytes()
                        } else {
                            unit.to_le_bytes()
                        }
                    })
                    .collect()
            };
            let mut body = encode("auth: ");
            body.extend(encode(secret));
            body.extend(encode(" end"));
            let stored = secrets.redact_bytes(&body);
            let mut expected = encode("auth: ");
            expected.extend_from_slice(REDACTED.as_bytes());
            expected.extend(encode(" end"));
            assert_eq!(stored, expected, "big-endian: {big}");
        }
    }
}

#[test]
fn a_secret_is_found_inside_base64_at_each_place_in_a_group_of_three() {
    // Base64 writes three bytes as four characters, so a secret's characters depend on
    // where in a group of three it begins: after nothing, after one byte, after two.
    // With `user:` before it, as a basic authorization header has it, it begins two
    // bytes into a group, and so sits across the edge of one.
    let key = made_up_key("");
    let secrets = secrets(&[&key]);
    for before in ["", "u", "us", "use", "user", "user:", "a longer text: "] {
        for after in ["", "!", "!!", "!!!", " and more after it"] {
            for url in [false, true] {
                let plain = format!("{before}{key}{after}");
                let encoded = base64(plain.as_bytes(), url);
                // The characters that hold any bit of the secret, from the first to
                // the last: every one is replaced, and no other.
                let first = before.len() * 8 / 6;
                let last = ((before.len() + key.len()) * 8 - 1) / 6;
                let expected = format!(
                    "Basic {}[redacted]{} x",
                    &encoded[..first],
                    &encoded[last + 1..]
                );
                let stored = secrets.redact(&format!("Basic {encoded} x"));
                assert_eq!(stored, expected, "{before:?} {after:?} {url}");
            }
        }
    }
    // A secret whose base64 holds `+` or `/` is found in both alphabets.
    let odd = "k\u{fb}\u{ff}\u{fe}~~>>??zz".to_owned();
    let secrets = Secrets::new([odd.clone()]).unwrap();
    let (standard, url) = (base64(odd.as_bytes(), false), base64(odd.as_bytes(), true));
    assert!(standard.contains('+') || standard.contains('/'));
    assert_ne!(standard, url);
    assert_eq!(secrets.redact(&standard), "[redacted]");
    assert_eq!(secrets.redact(&url), "[redacted]");
}

#[test]
fn the_forms_are_found_inside_one_another() {
    // The forms are read, so one inside another is found where the reading reaches:
    // base64 inside a JSON string that escapes its slashes, base64 percent-encoded in
    // a URL, and a `\u` escape inside percent-encoding.
    let odd = "k\u{fb}\u{ff}\u{fe}~~>>??zz".to_owned();
    let key = made_up_key("");
    let secrets = Secrets::new([odd.clone(), key.clone()]).unwrap();
    let encoded = base64(odd.as_bytes(), false);
    assert!(encoded.contains('/') && encoded.contains('+'), "{encoded}");
    let in_json = format!("{{\"h\":\"{}\"}}", encoded.replace('/', "\\/"));
    assert_eq!(secrets.redact(&in_json), "{\"h\":\"[redacted]\"}");
    let in_url = format!(
        "?h={}&x=1",
        encoded
            .replace('+', "%2B")
            .replace('/', "%2F")
            .replace('=', "%3D")
    );
    assert_eq!(secrets.redact(&in_url), "?h=[redacted]&x=1");
    let both = key.replacen('a', "%5Cu0061", 1);
    assert_eq!(
        secrets.redact(&format!("?k={both}&x=1")),
        "?k=[redacted]&x=1"
    );
}

#[test]
fn what_redaction_does_not_find_is_known() {
    // ADR 0013, "What is accepted". Each of these gets through, with its reason in the
    // record, and is pinned so that the list there stays true.
    let key = made_up_key("");
    let secrets = secrets(&[&key]);
    // Percent-encoded three times: the reading goes two deep.
    let odd = Secrets::new(["ab cd/ef+gh=ij".to_owned()]).unwrap();
    let thrice = "k=ab%252520cd%25252Fef%25252Bgh%25253Dij";
    assert_eq!(odd.redact(thrice), thrice);
    // A `+` for a space inside a value that is encoded twice.
    let plus_twice = "k=ab+cd%252Fef%252Bgh%253Dij";
    assert_eq!(odd.redact(plus_twice), plus_twice);
    // The letters in the other case, and the key written as hex digits.
    let upper = key.to_uppercase();
    assert_eq!(secrets.redact(&upper), upper);
    let hex: String = key.bytes().map(|byte| format!("{byte:02x}")).collect();
    assert_eq!(secrets.redact(&hex), hex);
    // UTF-32, and base64 of UTF-16, as a shell's encoded command holds a text.
    let wide: Vec<u8> = key.chars().flat_map(|c| (c as u32).to_le_bytes()).collect();
    assert_eq!(secrets.redact_bytes(&wide), wide);
    let utf16: Vec<u8> = key.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let encoded = base64(&utf16, false);
    assert_eq!(secrets.redact(&encoded), encoded);
    // Base64 with a line break inside the key's characters, as a wrapped body has it.
    let wrapped = base64(format!("user:{key}").as_bytes(), false);
    let wrapped = format!("{}\r\n{}", &wrapped[..16], &wrapped[16..]);
    assert_eq!(secrets.redact(&wrapped), wrapped);
    // A key split across two texts.
    let (head, tail) = key.split_at(key.len() / 2);
    assert_eq!(secrets.redact(head), head);
    assert_eq!(secrets.redact(tail), tail);
    // A body searched as bytes is not read as JSON: a secret that begins with a letter
    // an escape ends in, behind a backslash, leaves the backslash in front of the
    // marker, and the body is no longer JSON.
    let after = Secrets::new(["nabcdefgh".to_owned()]).unwrap();
    let body = serde_json::json!({ "t": "x\nabcdefgh" }).to_string();
    let stored = after.redact(&body);
    assert_eq!(stored, r#"{"t":"x\[redacted]"}"#);
    assert!(serde_json::from_str::<serde_json::Value>(&stored).is_err());
}

#[test]
fn many_secrets_that_share_their_start_cost_no_more_than_one() {
    // The case ADR 0005 measured at about 3 seconds: 1 MiB of JSON against 2,000 keys
    // that share their first letters, where the text is full of those letters. The
    // search follows the text once, whatever the number of secrets, so it is asked to
    // finish inside a bound far under that; what it took is in ADR 0013.
    let keys: Vec<String> = (0..2000)
        .map(|n| made_up_key(&format!("-{n:04}")))
        .collect();
    let secrets = Secrets::new(keys.clone()).unwrap();
    let shared = &keys[0][..keys[0].len() - 4];
    let mut body = String::from("{\"lines\":[");
    while body.len() < 1024 * 1024 {
        body.push_str(&format!("\"{shared}\",\"x\\n{shared}zzzz\","));
    }
    body.push_str(&format!("\"{}\"]}}", keys[1999]));
    let started = std::time::Instant::now();
    let stored = secrets.redact(&body);
    let took = started.elapsed();
    assert!(stored.ends_with("\"[redacted]\"]}"));
    assert_eq!(stored.len(), body.len() - keys[1999].len() + REDACTED.len());
    assert!(took < std::time::Duration::from_secs(10), "{took:?}");
}

#[test]
fn a_mebibyte_with_ten_secrets_is_searched_inside_a_stated_time() {
    // Plan 1B, task C3: the pass over 1 MiB with ten secrets, in a body that makes
    // every reading run (it holds `%`, `\` and `+`). The bound is loose, for a busy
    // machine and an unoptimised build; what it took on a quiet one is in ADR 0013.
    let keys: Vec<String> = (0..10).map(|n| made_up_key(&format!("-{n}"))).collect();
    let secrets = Secrets::new(keys.clone()).unwrap();
    let mut body = String::new();
    while body.len() < 1024 * 1024 {
        body.push_str("{\"u\":\"https:\\/\\/h.example\\/a%20b+c?q=1\",\"n\":\"line\\nline\"}\n");
    }
    for key in &keys {
        body.push_str(&format!("auth={}&\n", key.replacen('a', "%61", 1)));
    }
    let started = std::time::Instant::now();
    let stored = secrets.redact(&body);
    let took = started.elapsed();
    assert_eq!(stored.matches(REDACTED).count(), 10);
    assert!(took < std::time::Duration::from_secs(10), "{took:?}");
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
