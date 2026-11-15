//! Redaction (`SPEC.md` section 3: "Secrets are redacted before anything is written";
//! ADR 0005, "Redaction"; ADR 0014, which changes what is found). The pass began as squint's
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
    // ADR 0014: percent-encoding is read, not matched against one written form. The
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
fn two_forms_that_end_at_one_place_are_both_found() {
    // One secret is, as it stands, some letters and then the base64 characters of
    // another. Where the two end together, the longer is found first; the shorter is
    // still found, and with it the base64 character after it, which holds the last
    // bits of the second secret beside its neighbour's.
    let key = made_up_key("x");
    assert_eq!(key.len() * 8 % 6, 2, "its bits do not end a character");
    let encoded = base64(format!("{key}zz").as_bytes(), false);
    let whole = key.len() * 8 / 6;
    let longer = format!("PRE{}", &encoded[..whole]);
    let secrets = Secrets::new([key.clone(), longer]).unwrap();
    assert_eq!(
        secrets.redact(&format!("PRE{encoded}")),
        format!("[redacted]{}", &encoded[whole + 1..])
    );
}

#[test]
fn values_an_env_file_keeps_are_found_in_every_form_whatever_marks_they_hold() {
    // An env file's reader keeps a value with a mark that is white space and not
    // ASCII at an end (a no-break space, a line separator, an ideographic space), with
    // `$(...)`, `#`, `=`, and with a quote of the other kind. A set of secrets trims
    // ASCII white space only, so each is searched for as the file gave it.
    let values = [
        "\u{a0}zqxjkvwpyh-0123\u{a0}".to_owned(),
        "zqxjkvwpyh-4567\u{2028}".to_owned(),
        "\u{3000}zqxjkvwpyh-89ab".to_owned(),
        "zq$(xjk)vw#py=h+0/1%2".to_owned(),
        "zqxj\"kvw'pyh\\0123".to_owned(),
    ];
    let file = format!(
        "A={}\nB={}\nC={}\nD={}\nE='zqxj\"kvw'\nF=\"kvw'pyh-0123\"\n",
        values[0], values[1], values[2], values[3]
    );
    let env = whizbang_core::config::EnvFile::parse(file.as_bytes()).unwrap();
    assert_eq!(format!("{:?}", env.secrets()), "Secrets(6)");
    let from_file = env.secrets();
    let told = Secrets::new(values.clone()).unwrap();
    for (index, value) in values.iter().enumerate() {
        let secrets = if index < 4 { &from_file } else { &told };
        let percent: String = value.bytes().map(|byte| format!("%{byte:02X}")).collect();
        let escaped: String = value
            .chars()
            .map(|c| {
                let mut units = [0u16; 2];
                c.encode_utf16(&mut units)
                    .iter()
                    .map(|unit| format!("\\u{unit:04x}"))
                    .collect::<String>()
            })
            .collect();
        let quoted = serde_json::to_string(value).unwrap();
        for form in [
            value.clone(),
            percent,
            escaped,
            quoted[1..quoted.len() - 1].to_owned(),
            base64(value.as_bytes(), false),
            base64(format!("x:{value}").as_bytes(), true),
        ] {
            let stored = secrets.redact(&format!("<{form}>"));
            assert!(stored.contains(REDACTED), "{index}: {form}");
            assert!(
                !stored.contains("zqxjkvwpyh") && !stored.contains("kvw'pyh"),
                "{stored}"
            );
        }
        let wide: Vec<u8> = value.encode_utf16().flat_map(u16::to_be_bytes).collect();
        assert_eq!(secrets.redact_bytes(&wide), REDACTED.as_bytes(), "{index}");
    }
    // The two quoted values of the file, each with a quote of the other kind.
    assert_eq!(
        from_file.redact("a zqxj\"kvw b kvw'pyh-0123 c"),
        "a [redacted] b [redacted] c"
    );
}

// --- the ways a secret is written, for the tests below --------------------------------

/// As a JSON string escapes it, without the quotes around it.
fn json_of(text: &str) -> String {
    let quoted = serde_json::to_string(text).unwrap();
    quoted[1..quoted.len() - 1].to_owned()
}

/// Percent-encoded: every byte, or every byte that is not a letter, a digit, `-`, `.`,
/// `_` or `~`.
fn pct(text: &str, upper: bool, all: bool) -> String {
    text.bytes()
        .map(|byte| {
            let plain = byte.is_ascii_alphanumeric() || b"-._~".contains(&byte);
            if plain && !all {
                char::from(byte).to_string()
            } else if upper {
                format!("%{byte:02X}")
            } else {
                format!("%{byte:02x}")
            }
        })
        .collect()
}

/// Every character as `\u` escapes.
fn u_all(text: &str) -> String {
    text.encode_utf16()
        .map(|unit| format!("\\u{unit:04x}"))
        .collect()
}

fn utf16(text: &str, big: bool) -> Vec<u8> {
    text.encode_utf16()
        .flat_map(|unit| {
            if big {
                unit.to_be_bytes()
            } else {
                unit.to_le_bytes()
            }
        })
        .collect()
}

/// The first and the last base64 character that hold a bit of a secret that stands
/// after `before` bytes.
fn base64_edges(before: usize, secret: usize) -> (usize, usize) {
    (before * 8 / 6, ((before + secret) * 8 - 1) / 6)
}

#[test]
fn a_secret_whose_own_text_holds_an_encoding_is_found_however_it_is_written() {
    // A secret can hold a `%` and two hex digits, or a backslash and a letter, as its
    // own characters. Read as percent-encoding and as JSON escapes at once, such a
    // secret written as JSON escapes it would be decoded into something else. So the
    // three written forms of ADR 0005 are searched for as they are, and each kind of
    // encoding is also read alone.
    for secret in [
        "p%40ss/word-12",
        "sig=ab%2Bcd%3D&exp=12",
        "100%/done\\now",
        "a\\u0041b%zz/c9",
        "%41%42%43%44/x",
    ] {
        let secrets = Secrets::new([secret.to_owned()]).unwrap();
        let json = json_of(secret);
        for written in [
            secret.to_owned(),
            json.clone(),
            json.replace('/', "\\/"),
            // As a writer that escapes `&` gives it.
            json.replace('&', "\\u0026"),
            json_of(&json),
            pct(secret, true, false),
            pct(secret, false, false),
            pct(secret, true, true),
            pct(&pct(secret, true, false), true, false),
            u_all(secret),
        ] {
            assert_eq!(
                secrets.redact(&format!("<{written}>")),
                "<[redacted]>",
                "{secret} as {written}"
            );
        }
    }
    // An address with an encoded password, in a JSON string that escapes its slashes.
    // (It is put together here from its parts.)
    let address = format!("postgres://user{}p%40ss-w0rd{}db.example/main", ":", "@");
    let secrets = Secrets::new([address.clone()]).unwrap();
    let body = format!("{{\"dsn\":\"{}\"}}", json_of(&address).replace('/', "\\/"));
    assert_eq!(secrets.redact(&body), "{\"dsn\":\"[redacted]\"}");
}

#[test]
fn each_reading_finds_a_text_that_no_other_reading_does() {
    // The secret holds, as its own letters, a backslash and `n`, and a `%` and two hex
    // digits: read once more than it was written, either becomes something else. So
    // each way of writing it below is found by one reading and by no other.
    let secret = "a\\nb%41c/def";
    assert_eq!(secret.len(), 12);
    let secrets = Secrets::new([secret.to_owned()]).unwrap();
    let both = "%61%5Cn%62\\u002541c/def";
    for (reading, written) in [
        // Every character an escape.
        ("escapes", u_all(secret)),
        // Every byte percent-encoded.
        ("percent", pct(secret, true, true)),
        // Escapes, and then percent-encoding that the escapes hid.
        (
            "escapes, then percent",
            "a\\\\nb\\u00252541c\\u00252Fd\\u002565f".to_owned(),
        ),
        // The two mixed at one level, where each alone would misread the other's.
        ("both at once", both.to_owned()),
        // The same, written once more: each `%` percent-encoded, the backslash escaped.
        ("both, twice", "%2561%255Cn%2562\\\\u002541c/def".to_owned()),
    ] {
        assert_eq!(
            secrets.redact(&format!("<{written}>")),
            "<[redacted]>",
            "{reading}: {written}"
        );
    }
}

#[test]
fn after_a_text_that_ends_in_a_percent_or_a_backslash_a_written_form_is_still_found() {
    // The secret begins with two hex digits, and with a letter an escape ends in.
    for secret in ["ab12cd34/ef+gh", "nb12cd34/ef+gh"] {
        let secrets = Secrets::new([secret.to_owned()]).unwrap();
        for before in ["%", "\\", "100%", "C:\\", "%%", "\\\\"] {
            let json = json_of(secret);
            for written in [
                secret.to_owned(),
                json.clone(),
                json.replace('/', "\\/"),
                pct(secret, true, false),
                pct(secret, false, false),
            ] {
                assert_eq!(
                    secrets.redact(&format!("{before}{written};")),
                    format!("{before}[redacted];"),
                    "{before} {written}"
                );
            }
            let encoded = base64(secret.as_bytes(), false);
            let (_, last) = base64_edges(0, secret.len());
            assert_eq!(
                secrets.redact(&format!("{before}{encoded}")),
                format!("{before}[redacted]{}", &encoded[last + 1..])
            );
            let mut wide = before.as_bytes().to_vec();
            wide.extend(utf16(secret, false));
            let mut expected = before.as_bytes().to_vec();
            expected.extend_from_slice(REDACTED.as_bytes());
            assert_eq!(secrets.redact_bytes(&wide), expected, "{before}");
        }
    }
}

#[test]
fn a_form_of_json_after_a_lone_backslash_is_found_by_its_written_form_alone() {
    // The secret begins with a letter an escape ends in, and holds a quote and a
    // slash. Written as a JSON string escapes it, with the slash left plain, straight
    // after a lone backslash, no reading finds it: each takes the backslash with the
    // letter. The written form is found as it stands. (With no slash, the form that
    // escapes slashes is the same bytes, and would find it.)
    let secret = "nab\"cd/efgh12";
    let secrets = Secrets::new([secret.to_owned()]).unwrap();
    assert_eq!(
        secrets.redact(&format!("x\\{};", json_of(secret))),
        "x\\[redacted];"
    );
}

#[test]
fn the_character_before_a_base64_form_is_taken_only_when_it_is_base64_and_the_forms_own() {
    let key = made_up_key("x");
    let secrets = secrets(&[&key]);
    // The run of whole characters of a secret that does not begin a group, straight
    // after a quote: the quote holds no bit of the secret, and stays.
    let encoded = base64(format!("u{key}!!").as_bytes(), false);
    let (first, last) = base64_edges(1, key.len());
    assert_eq!(
        secrets.redact(&format!("{{\"h\":\"{}\"}}", &encoded[first + 1..])),
        format!("{{\"h\":\"[redacted]{}\"}}", &encoded[last + 1..])
    );
    // A longer form that ends where a shorter one does has its own edge, and not the
    // shorter one's. The longer here is a secret that is some letters and then the
    // base64 characters of the first: the letter before it is no part of either.
    let run = &encoded[first + 1..=last - 1];
    let longer = format!("PRE{run}");
    let both = Secrets::new([key.clone(), longer.clone()]).unwrap();
    assert_eq!(both.redact(&format!("x{longer};")), "x[redacted];");
    // And a longer base64 form: the secret with a letter before it begins a group
    // where the secret alone does not. The character before the longer one's run is
    // not its, as it is and where a line break stands inside the run.
    let longer = format!("u{key}");
    let both = Secrets::new([key.clone(), longer]).unwrap();
    assert_eq!(
        both.redact(&format!("x{encoded};")),
        format!("x[redacted]{};", &encoded[last + 1..])
    );
    let wrapped = format!("x{}\n{};", &encoded[..20], &encoded[20..]);
    assert_eq!(
        both.redact(&wrapped),
        format!("x[redacted]{};", &encoded[last + 1..])
    );
}

#[test]
fn a_secret_that_is_anothers_utf_16_bytes_is_never_left_out_for_a_twin() {
    // One secret, as text, is the bytes of another in UTF-16. The two are one form
    // of the machine. As the second's wide form it could be left out at an odd place,
    // for its twin a byte to the side; as the first secret as it stands it is never
    // left out.
    let narrow = "abcdefgh";
    let as_text: String = narrow.chars().flat_map(|c| [c, '\0']).collect();
    assert_eq!(as_text.as_bytes(), utf16(narrow, false));
    let secrets = Secrets::new([narrow.to_owned(), as_text.clone()]).unwrap();
    let text = [&[0u8][..], as_text.as_bytes()].concat();
    assert_eq!(secrets.redact_bytes(&text), b"[redacted]");
}

#[test]
fn a_second_secret_uncovers_nothing_of_the_first() {
    // The second secret is the first from its second letter on. The first stands in
    // UTF-16, little-endian, at an odd place; the second is then found one byte on,
    // big-endian, at an even place. That is another secret's stretch, and takes
    // nothing from the first's.
    let first = made_up_key("");
    let second = first[1..].to_owned();
    let mut text = b"X".to_vec();
    text.extend(utf16(&first, false));
    for secrets in [
        Secrets::new([first.clone()]).unwrap(),
        Secrets::new([first.clone(), second.clone()]).unwrap(),
    ] {
        assert_eq!(secrets.redact_bytes(&text), b"X[redacted]");
    }
    // For pairs made from one secret, and each way of writing the first: whatever of
    // it is gone with the first alone told is gone with both told.
    let mut rng = Rng(0x5eed_0002);
    for _ in 0..24 {
        let first = rng.secret();
        let others = [
            first[first.char_indices().nth(1).unwrap().0..].to_owned(),
            first[..first.char_indices().nth_back(0).unwrap().0].to_owned(),
            format!("x{first}"),
            rng.secret(),
        ];
        for other in others {
            if other.trim().len() < 8 {
                continue;
            }
            let alone = Secrets::new([first.clone()]).unwrap();
            let both = Secrets::new([first.clone(), other.clone()]).unwrap();
            for (written, _) in written_forms(&first) {
                for before in [&b""[..], b"X", b"auth: "] {
                    let text = [before, &written[..], b" end"].concat();
                    let (alone, both) = (alone.redact_bytes(&text), both.redact_bytes(&text));
                    for slice in written.windows(6) {
                        if !holds(&alone, slice) {
                            assert!(!holds(&both, slice), "{first:?} with {other:?}");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn base64_inside_another_form_loses_every_character_that_holds_a_bit_of_the_secret() {
    // The base64 of a secret, with its characters written again: `/` as `\/`, as a
    // JSON writer may; `+`, `/` and `=` percent-encoded, as an address must; every
    // character percent-encoded; every character a `\u` escape. The characters at
    // the secret's two edges hold two or four of its bits, and each is replaced
    // whole, however it is written: no half of an escape is left.
    let odd = "k\u{fb}\u{ff}\u{fe}~~>>??zz";
    let secrets = Secrets::new([odd.to_owned()]).unwrap();
    type Way = fn(char) -> String;
    let ways: [(&str, Way); 4] = [
        ("json", |c| {
            if c == '/' {
                "\\/".to_owned()
            } else {
                c.to_string()
            }
        }),
        ("percent of what must be", |c| {
            if "+/=".contains(c) {
                format!("%{:02X}", c as u32)
            } else {
                c.to_string()
            }
        }),
        ("percent of all", |c| format!("%{:02x}", c as u32)),
        ("escapes of all", |c| format!("\\u{:04x}", c as u32)),
    ];
    for before in ["", "u", "us", "/", "//", "user:"] {
        for after in ["", "/", "!!", "//?"] {
            for url in [false, true] {
                let plain = format!("{before}{odd}{after}");
                let encoded = base64(plain.as_bytes(), url);
                let (first, last) = base64_edges(before.len(), odd.len());
                for (name, way) in ways {
                    let write = |part: &str| part.chars().map(way).collect::<String>();
                    assert_eq!(
                        secrets.redact(&format!("<{}>", write(&encoded))),
                        format!(
                            "<{}[redacted]{}>",
                            write(&encoded[..first]),
                            write(&encoded[last + 1..])
                        ),
                        "{name}: {before:?} {after:?} {url}"
                    );
                }
            }
        }
    }
}

#[test]
fn base64_broken_by_line_breaks_is_found_and_ordinary_lines_are_not_joined() {
    // A long body in base64 is wrapped, at 76 columns by one tool and at 64 by
    // another, with a line feed or a carriage return and a line feed. A line break
    // between two base64 characters is passed over, for base64 only.
    let key = made_up_key(&"z".repeat(40));
    assert!(key.len() > 57);
    let secrets = secrets(&[&key]);
    let encoded = base64(format!("user:{key}").as_bytes(), false);
    let (first, last) = base64_edges(5, key.len());
    for width in [76, 64] {
        for (breaks, wide) in [("\n", 1), ("\r\n", 2), ("\r", 1)] {
            let lines: Vec<&str> = encoded
                .as_bytes()
                .chunks(width)
                .map(|line| std::str::from_utf8(line).unwrap())
                .collect();
            let wrapped = lines.join(breaks);
            let at = |index: usize| index + index / width * wide;
            assert!(lines.len() > 1 && at(last) > at(first) + key.len());
            assert_eq!(
                secrets.redact(&format!("-----\n{wrapped}\n-----")),
                format!(
                    "-----\n{}[redacted]{}\n-----",
                    &wrapped[..at(first)],
                    &wrapped[at(last) + 1..]
                ),
                "{width} {breaks:?}"
            );
            // The same inside a JSON string, where a line break is an escape.
            let escaped = json_of(&wrapped);
            let at = |index: usize| index + index / width * wide * 2;
            assert_eq!(
                secrets.redact(&format!("{{\"pem\":\"{escaped}\"}}")),
                format!(
                    "{{\"pem\":\"{}[redacted]{}\"}}",
                    &escaped[..at(first)],
                    &escaped[at(last) + 1..]
                ),
                "{width} {breaks:?} in JSON"
            );
        }
    }
    // A secret that is not base64 is not found across a line break: two lines of an
    // ordinary text are two texts.
    let plain = Secrets::new(["abcdefgh12345678".to_owned()]).unwrap();
    for text in [
        "abcdefgh\n12345678",
        "abcdefgh\r\n12345678",
        "{\"t\":\"abcdefgh\\n12345678\"}",
    ] {
        assert_eq!(plain.redact(text), text);
    }
}

#[test]
fn two_hundred_secrets_that_end_inside_one_another_cost_no_more_than_one() {
    // Every place in the text ends two hundred secrets. One stretch is noted for a
    // place, the longest, and not one for each: noted each, a mebibyte of this was
    // some two hundred million stretches.
    let secrets = Secrets::new((8..208).map(|length| "a".repeat(length))).unwrap();
    let body = "a".repeat(1024 * 1024);
    let started = std::time::Instant::now();
    let stored = secrets.redact(&body);
    let took = started.elapsed();
    assert_eq!(stored, "[redacted]");
    assert!(took < std::time::Duration::from_secs(10), "{took:?}");
    // The same in UTF-16, and with a rest that is no secret.
    let mut wide = utf16(&"a".repeat(64 * 1024), false);
    wide.extend_from_slice(b"\x01\x02");
    assert_eq!(secrets.redact_bytes(&wide), b"[redacted]\x01\x02");
}

#[test]
fn bytes_that_were_text_can_come_back_as_no_text_and_a_text_comes_back_as_one() {
    // The secret's UTF-16 bytes stand inside the UTF-8 of other letters: taken out as
    // bytes, the halves of two letters are left. `redact_bytes` gives those bytes; a
    // text goes through `redact`, which gives a text, with the mark for what is no
    // character where a half stood.
    let secret = "\u{a9c3}\u{a9c3}\u{a9c3}";
    let secrets = Secrets::new([secret.to_owned()]).unwrap();
    let text = "X\u{e9}\u{e9}\u{e9}\u{e9}";
    let bytes = secrets.redact_bytes(text.as_bytes());
    assert_eq!(bytes, [&[b'X', 0xc3][..], b"[redacted]", &[0xa9]].concat());
    assert!(std::str::from_utf8(&bytes).is_err());
    assert_eq!(secrets.redact(text), "X\u{fffd}[redacted]\u{fffd}");
}

/// Numbers that are the same on every run: xorshift, from a seed.
struct Rng(u64);

impl Rng {
    fn below(&mut self, bound: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % bound as u64) as usize
    }

    /// A made-up secret of 8 to 20 pieces, among them the marks an encoding is made
    /// of, with no space at an end.
    fn secret(&mut self) -> String {
        const PIECES: [&str; 26] = [
            "a", "b", "c", "d", "e", "f", "0", "1", "2", "9", "Z", "q", "n", "u", "%", "\\", "/",
            "+", "=", "\"", " ", "-", "_", "&", "\u{e9}", "~",
        ];
        let count = 8 + self.below(13);
        let made: String = (0..count).map(|_| PIECES[self.below(26)]).collect();
        format!("k{}k", made.trim())
    }
}

/// A secret in each way it is said to be found written whole, as bytes, with whether
/// the way is found after a text that ends in a `%` or a backslash.
fn written_forms(secret: &str) -> Vec<(Vec<u8>, bool)> {
    let json = json_of(secret);
    let mut forms: Vec<(Vec<u8>, bool)> = vec![
        (secret.as_bytes().to_vec(), true),
        (json.clone().into_bytes(), true),
        (json.replace('/', "\\/").into_bytes(), true),
        (pct(secret, true, false).into_bytes(), true),
        (pct(secret, false, false).into_bytes(), true),
        (pct(secret, true, true).into_bytes(), false),
        (pct(secret, false, true).into_bytes(), false),
        (
            pct(&pct(secret, true, false), false, false).into_bytes(),
            false,
        ),
        (u_all(secret).into_bytes(), false),
        (json_of(&json).into_bytes(), false),
        (utf16(secret, false), true),
        (utf16(secret, true), true),
    ];
    forms.dedup();
    forms
}

#[test]
fn a_generated_secret_in_each_form_at_each_place_is_replaced_exactly() {
    // Held for the next change to the search: secrets made from a seed, each written
    // every way it is said to be found, between texts of several kinds. What comes
    // back is the text before, the marker and the text after, exactly: nothing of the
    // secret is left in any encoding, and nothing beside it is taken.
    let mut rng = Rng(0x5eed_0001);
    let mut cases = 0;
    for _ in 0..24 {
        let secret = rng.secret();
        let secrets = Secrets::new([secret.clone()]).unwrap();
        let places: [(&str, &str, bool); 6] = [
            ("", "", false),
            ("auth: ", " end", false),
            ("{\"t\":\"", "\"}", false),
            ("?k=", "&x=1", false),
            ("100%", ";", true),
            ("C:\\", ";", true),
        ];
        for (before, after, hard) in places {
            for (written, after_a_mark) in written_forms(&secret) {
                if hard && !after_a_mark {
                    continue;
                }
                let text = [before.as_bytes(), &written[..], after.as_bytes()].concat();
                let expected = [before.as_bytes(), REDACTED.as_bytes(), after.as_bytes()].concat();
                assert_eq!(
                    String::from_utf8_lossy(&secrets.redact_bytes(&text)),
                    String::from_utf8_lossy(&expected),
                    "{secret:?} written {:?} after {before:?}",
                    String::from_utf8_lossy(&written)
                );
                cases += 1;
            }
            for lead in ["", "u", "us"] {
                for url in [false, true] {
                    let encoded = base64(format!("{lead}{secret}!!").as_bytes(), url);
                    let (first, last) = base64_edges(lead.len(), secret.len());
                    assert_eq!(
                        secrets.redact(&format!("{before}{encoded}{after}")),
                        format!(
                            "{before}{}[redacted]{}{after}",
                            &encoded[..first],
                            &encoded[last + 1..]
                        ),
                        "{secret:?} in base64 after {lead:?} and {before:?}"
                    );
                    cases += 1;
                }
            }
        }
    }
    assert!(cases > 1500, "{cases}");
}

#[test]
fn what_redaction_does_not_find_is_known() {
    // ADR 0014, "What is accepted". Each of these gets through, with its reason in the
    // record, and is pinned so that the list there stays true.
    let key = made_up_key("");
    let secrets = secrets(&[&key]);
    // Percent-encoded four times: the readings go two deep, and the written form they
    // then find is the third.
    let odd = Secrets::new(["ab cd/ef+gh=ij".to_owned()]).unwrap();
    let thrice = "k=ab%252520cd%25252Fef%25252Bgh%25253Dij";
    assert_eq!(odd.redact(thrice), "k=[redacted]");
    let four = thrice.replace('%', "%25");
    assert_eq!(odd.redact(&four), four);
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
    // Base64 broken by something other than one line break: two blank lines, or a
    // space, as a body folded for a header has it.
    let whole = base64(format!("user:{key}").as_bytes(), false);
    for gap in ["\n\n", "\r\n\r\n", "\n\r", "\r\n ", " "] {
        let folded = format!("{}{gap}{}", &whole[..16], &whole[16..]);
        assert_eq!(secrets.redact(&folded), folded, "{gap:?}");
    }
    // A reading goes through a text once, from its start. A `%` straight before a
    // secret that begins with two hex digits takes them for its own; where the secret
    // is written in a mixture that only a reading finds, it is then missed.
    let eaten = "%ab%20cd%2fef%2Bgh%3dij";
    assert_eq!(odd.redact("ab%20cd%2fef%2Bgh%3dij"), "[redacted]");
    assert_eq!(odd.redact(eaten), eaten);
    // A lone backslash straight before a secret written as `\u` escapes is read with
    // the first escape's backslash; the second reading then finds the secret, and the
    // lone backslash goes with it.
    let escaped = format!("\\{}", u_all(&key));
    assert_eq!(secrets.redact(&escaped), "[redacted]");
    // After a lone backslash, an encoded secret that only a reading finds can be read
    // wrongly: the backslash is taken with what follows it. Written as `\u` escapes
    // inside percent-encoding, it is missed.
    let inside = format!("\\{}", u_all(&key).replace('\\', "%5C"));
    assert_eq!(secrets.redact(&inside[1..]), "[redacted]");
    assert_eq!(secrets.redact(&inside), inside);
    // In mixed escapes, beginning with a letter an escape ends in, it is missed.
    let letter = Secrets::new(["nabcdefgh12".to_owned()]).unwrap();
    let mixed = format!("\\n{}", u_all("abcdefgh12"));
    assert_eq!(letter.redact(&mixed[1..]), "[redacted]");
    assert_eq!(letter.redact(&mixed), mixed);
    // Beginning with a digit it is found, and the last digit of the first escape goes
    // with it while the rest of that escape stays.
    let digit = Secrets::new(["1abcdefgh12".to_owned()]).unwrap();
    let escaped = format!("\\{}", u_all("1abcdefgh12").replace('\\', "%5C"));
    assert_eq!(digit.redact(&escaped), "\\%5Cu003[redacted]");
    // A secret that is the marker's own word, or part of it, is replaced inside the
    // marker when what was stored is redacted again: the brackets grow.
    let word = Secrets::new(["redacted".to_owned(), key.clone()]).unwrap();
    let once = word.redact(&format!("a {key} b"));
    assert_eq!(once, "a [redacted] b");
    assert_eq!(word.redact(&once), "a [[redacted]] b");
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
    // finish inside a bound far under that; what it took is in ADR 0014.
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
    // machine and an unoptimised build; what it took on a quiet one is in ADR 0014.
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

// --- a body that is JSON, redacted as JSON (ADR 0014, "A JSON body") --------------------

fn json(body: &str) -> serde_json::Value {
    serde_json::from_str(body).unwrap_or_else(|why| panic!("not JSON: {why}\n{body}"))
}

#[test]
fn a_json_body_with_a_secret_at_the_edge_of_an_escape_is_still_json_after_it() {
    // The two cases that break a body searched as bytes (ADR 0005): a secret that ends
    // in a backslash, in front of an escape that is not its own; and a secret that
    // begins with a letter an escape ends in, behind a backslash. Read as JSON, each
    // string is decoded before it is searched, so neither edge is there.
    let ends = "abcdefgh\\".to_owned();
    let begins = "nabcdefgh".to_owned();
    let secrets = Secrets::new([ends.clone(), begins.clone()]).unwrap();
    for (value, expected) in [
        (format!("{ends}\"quoted\""), "[redacted]\"quoted\""),
        (format!("x {ends}\n"), "x [redacted]\n"),
        (format!("x\n{}", &begins[1..]), "x\nabcdefgh"),
        (format!("x\\{begins}"), "x\\[redacted]"),
        (format!("{begins}{ends}"), "[redacted][redacted]"),
    ] {
        let body = serde_json::json!({ "t": value, "n": 1 }).to_string();
        let stored = String::from_utf8(secrets.redact_json(body.as_bytes())).unwrap();
        assert_eq!(
            json(&stored),
            serde_json::json!({ "t": expected, "n": 1 }),
            "{body}"
        );
    }
    // The same body searched as bytes is the case ADR 0014 accepts, and is not JSON.
    let body = serde_json::json!({ "t": "x\nabcdefgh" }).to_string();
    assert!(serde_json::from_str::<serde_json::Value>(&secrets.redact(&body)).is_err());
    assert_eq!(
        String::from_utf8(secrets.redact_json(body.as_bytes())).unwrap(),
        body,
        "the string is `x`, a line break and `abcdefgh`, and holds no secret"
    );
}

#[test]
fn a_secret_behind_any_escape_of_a_json_string_is_taken_out_of_a_json_body() {
    let key = made_up_key("");
    let secrets = secrets(&[&key]);
    let every: String = key
        .chars()
        .map(|c| format!("\\u{:04X}", c as u32))
        .collect();
    let one = key.replacen('a', "\\u0061", 1);
    let body = format!(
        "{{ \"a\" : \"Bearer {one}\",\n  \"{every}\" : [ \"{key}\", \"x\\/{key}\\ty\" ] }}"
    );
    let stored = String::from_utf8(secrets.redact_json(body.as_bytes())).unwrap();
    assert_eq!(
        stored,
        "{ \"a\" : \"Bearer [redacted]\",\n  \"[redacted]\" : [ \"[redacted]\", \"x/[redacted]\\ty\" ] }"
    );
    json(&stored);
    for start in 0..key.len() - 5 {
        assert!(!stored.contains(&key[start..start + 5]), "{stored}");
    }
    // A string inside a string, as a body that quotes a body has it: the inner body's
    // escapes are read by the readings every text gets.
    let inner = serde_json::json!({ "authorization": key }).to_string();
    let outer = serde_json::json!({ "request": inner }).to_string();
    let stored = String::from_utf8(secrets.redact_json(outer.as_bytes())).unwrap();
    assert_eq!(
        json(&stored),
        serde_json::json!({ "request": "{\"authorization\":\"[redacted]\"}" })
    );
}

#[test]
fn a_backslash_before_a_secret_in_a_json_body_is_a_character_and_stays() {
    // The cases in which a lone backslash throws a reading off, as a JSON body has
    // them: the backslash is written `\\`, the string is decoded before it is
    // searched, and the secret is found as it stands. The backslash stays.
    for (secret, written) in [
        // Beginning with a letter an escape ends in, and holding a quote.
        ("nab\"cdefgh12", "nab\\\"cdefgh12".to_owned()),
        // Written as `\u` escapes, and in mixed escapes.
        ("nabcdefgh12", u_all("nabcdefgh12")),
        ("nabcdefgh12", format!("n{}", u_all("abcdefgh12"))),
        // Beginning with a digit.
        ("1abcdefgh12", u_all("1abcdefgh12")),
        ("ab12cd34/ef+gh", "ab12cd34\\/ef+gh".to_owned()),
    ] {
        let secrets = Secrets::new([secret.to_owned()]).unwrap();
        for before in ["x\\\\", "\\\\", "100%", "C:\\\\dir\\\\"] {
            let body = format!("{{\"t\":\"{before}{written}\",\"n\":1}}");
            let stored = String::from_utf8(secrets.redact_json(body.as_bytes())).unwrap();
            assert_eq!(
                stored,
                format!("{{\"t\":\"{before}[redacted]\",\"n\":1}}"),
                "{body}"
            );
            json(&stored);
        }
    }
    // What is written back is a text, where bytes taken out as bytes would not be.
    let wide = "\u{a9c3}\u{a9c3}\u{a9c3}";
    let secrets = Secrets::new([wide.to_owned()]).unwrap();
    let body = "{\"t\":\"X\u{e9}\u{e9}\u{e9}\u{e9}\"}";
    let stored = String::from_utf8(secrets.redact_json(body.as_bytes())).unwrap();
    assert_eq!(stored, "{\"t\":\"X\u{fffd}[redacted]\u{fffd}\"}");
    json(&stored);
}

#[test]
fn what_holds_no_secret_in_a_json_body_is_left_byte_for_byte() {
    let key = made_up_key("");
    let secrets = secrets(&[&key]);
    // Spacing, the order of keys, a key given twice, escapes a writer need not have
    // used, numbers as they were written: none is touched where no secret is.
    let before = "{\n\t\"b\" : 1.50e+3,\r\n \"a\":[true,false,null,-0, \"\\u0041\\/\\u00e9 \\ud83d\\udd11\"],\"a\":{} ,\"s\":\"";
    let after = "\" }  \n";
    let body = format!("{before}{key}{after}");
    let stored = String::from_utf8(secrets.redact_json(body.as_bytes())).unwrap();
    assert_eq!(stored, format!("{before}[redacted]{after}"));
    let none = format!("{before}plain{after}");
    assert_eq!(secrets.redact_json(none.as_bytes()), none.as_bytes());
    // A body that is one string, one number or nothing but a word.
    assert_eq!(
        secrets.redact_json(format!("\"{key}\"").as_bytes()),
        b"\"[redacted]\""
    );
    assert_eq!(secrets.redact_json(b" 12 "), b" 12 ");
    assert_eq!(secrets.redact_json(b"null"), b"null");
    assert_eq!(Secrets::none().redact_json(b"{\"a\":1}"), b"{\"a\":1}");
}

#[test]
fn a_number_that_holds_a_secret_becomes_a_string_and_the_body_is_still_json() {
    let secrets = secrets(&["1234567890", "truefalse"]);
    let body = "{\"id\":1234567890,\"big\":-91234567890.5e3,\"ok\":123456789,\"t\":true}";
    let stored = String::from_utf8(secrets.redact_json(body.as_bytes())).unwrap();
    assert_eq!(
        stored,
        "{\"id\":\"[redacted]\",\"big\":\"[redacted]\",\"ok\":123456789,\"t\":true}"
    );
    json(&stored);
}

#[test]
fn a_body_that_is_not_json_is_redacted_as_bytes_and_not_refused() {
    let key = made_up_key("");
    let secrets = secrets(&[&key]);
    for body in [
        format!("{{\"t\":\"{key}\""),
        format!("{{\"t\":\"{key}\"}} trailing"),
        format!("{{'t':'{key}'}}"),
        format!("not json at all: {key}"),
        format!("{{\"t\":\"{key}\",}}"),
        String::new(),
    ] {
        assert_eq!(
            secrets.redact_json(body.as_bytes()),
            secrets.redact_bytes(body.as_bytes()),
            "{}",
            body.replace(&key, "<key>")
        );
        assert!(
            !String::from_utf8(secrets.redact_json(body.as_bytes()))
                .unwrap()
                .contains(&key)
        );
    }
    // Bytes that are not text at all.
    let mut bytes = vec![0xff, b'{'];
    bytes.extend_from_slice(key.as_bytes());
    assert_eq!(
        secrets.redact_json(&bytes),
        [&[0xff, b'{'][..], b"[redacted]"].concat()
    );
    // Nesting is no reason: `a_body_nested_however_deep_is_read_as_json`. The key is out.
    let deep = format!("{}\"{key}\"{}", "[".repeat(200), "]".repeat(200));
    let stored = String::from_utf8(secrets.redact_json(deep.as_bytes())).unwrap();
    assert_eq!(
        stored,
        format!("{}\"[redacted]\"{}", "[".repeat(200), "]".repeat(200))
    );
}

#[test]
fn a_string_that_is_json_and_no_text_is_searched_as_it_is_written() {
    // Half of a pair of escapes is a string to JSON's grammar and no text. It is not
    // decoded; it is searched as bytes, and replaced whole when it holds a secret, so
    // that the body is still JSON.
    let key = made_up_key("");
    let secrets = secrets(&[&key]);
    let body = format!("{{\"a\":\"\\ud83d {key}\",\"b\":\"\\ud83d plain\"}}");
    let stored = String::from_utf8(secrets.redact_json(body.as_bytes())).unwrap();
    assert_eq!(stored, "{\"a\":\"[redacted]\",\"b\":\"\\ud83d plain\"}");
}

#[test]
fn what_redaction_of_a_json_body_does_not_find_is_known() {
    // ADR 0014, "A JSON body". A secret that stands across two strings, or across a
    // string and what is between strings, is in no one string: read as JSON it is not
    // found, where the same bytes searched as bytes would be.
    let across = "abcd\",\"efgh".to_owned();
    let secrets = Secrets::new([across.clone()]).unwrap();
    let body = "[\"abcd\",\"efgh\"]";
    assert_eq!(secrets.redact_json(body.as_bytes()), body.as_bytes());
    assert_eq!(secrets.redact(body), "[\"[redacted]\"]");
}

#[test]
fn a_mebibyte_of_json_with_ten_secrets_is_redacted_as_json_inside_a_stated_time() {
    let keys: Vec<String> = (0..10).map(|n| made_up_key(&format!("-{n}"))).collect();
    let secrets = Secrets::new(keys.clone()).unwrap();
    let mut body = String::from("[");
    while body.len() < 1024 * 1024 {
        body.push_str(
            "{\"u\":\"https:\\/\\/h.example\\/a%20b+c?q=1\",\"n\":\"line\\nline\",\"i\":12345},",
        );
    }
    for key in &keys {
        body.push_str(&format!("\"auth {}\",", key.replacen('a', "\\u0061", 1)));
    }
    body.push_str("0]");
    let started = std::time::Instant::now();
    let stored = String::from_utf8(secrets.redact_json(body.as_bytes())).unwrap();
    let took = started.elapsed();
    assert_eq!(stored.matches(REDACTED).count(), 10);
    json(&stored);
    assert!(took < std::time::Duration::from_secs(10), "{took:?}");
}

// --- what the review of that part asked for (ADR 0014, the third note) --------------------

#[test]
fn a_body_that_is_one_bare_number_that_holds_a_secret_becomes_the_marker() {
    // A number that is the whole body ends where the body does: nothing that stands
    // between values follows it. Cut anywhere short of that, the secret is in no piece.
    let secrets = secrets(&["1234567890"]);
    for body in ["991234567890", "-1234567890.25e3", "1234567890"] {
        let stored = String::from_utf8(secrets.redact_json(body.as_bytes())).unwrap();
        assert_eq!(stored, "\"[redacted]\"", "{body}");
        json(&stored);
    }
    assert_eq!(secrets.redact_json(b"123456789"), b"123456789");
}

/// Percent-encoded, every byte, with the case of the hex digits changing from one byte
/// to the next.
fn pct_mixed(text: &str) -> String {
    text.bytes()
        .enumerate()
        .map(|(at, byte)| {
            if at % 2 == 0 {
                format!("%{byte:02X}")
            } else {
                format!("%{byte:02x}")
            }
        })
        .collect()
}

#[test]
fn a_secret_found_only_by_decoding_a_string_is_taken_out_of_a_json_body() {
    // Percent-encoded twice in mixed case, and then written as `\u` escapes. Searched as
    // bytes, the escapes are one level and the two encodings two more, which is one
    // more than the readings go: nothing is found. Read as JSON the escapes are the
    // body's own and are decoded first; the text the string means is two levels deep.
    let secret = "jklmnopq/z+o";
    let secrets = secrets(&[secret]);
    let once = pct_mixed(secret);
    assert!(
        once.contains("%6A") && once.contains("%6b"),
        "both cases of hex"
    );
    let twice = pct_mixed(&once);
    assert_eq!(secrets.redact(&twice), "[redacted]");
    let body = format!("{{\"t\":\"{}\",\"n\":1}}", u_all(&twice));
    assert_eq!(
        secrets.redact_bytes(body.as_bytes()),
        body.as_bytes(),
        "searched as bytes it is not found: the case is one only decoding finds"
    );
    let stored = String::from_utf8(secrets.redact_json(body.as_bytes())).unwrap();
    assert_eq!(stored, "{\"t\":\"[redacted]\",\"n\":1}");
}

#[test]
fn a_string_of_a_json_body_that_is_not_utf_8_is_searched_as_it_is_written() {
    // The body is taken as JSON: the reader that is asked does not look inside a
    // string for text. The string is no text, so it is searched as it is written and
    // replaced whole when it holds a secret. What comes back is UTF-8 only when what
    // was given was.
    let key = made_up_key("");
    let secrets = secrets(&[&key]);
    let body = [
        b"{\"a\":\"\xff ",
        key.as_bytes(),
        b"\",\"b\":\"\xff plain\",\"c\":\"",
    ]
    .concat();
    let body = [&body[..], key.as_bytes(), b"\"}"].concat();
    assert!(std::str::from_utf8(&body).is_err());
    let stored = secrets.redact_json(&body);
    assert_eq!(
        stored,
        b"{\"a\":\"[redacted]\",\"b\":\"\xff plain\",\"c\":\"[redacted]\"}"
    );
    assert!(std::str::from_utf8(&stored).is_err());
    // Searched as bytes, the first string would have kept what stood before the key.
    assert_ne!(stored, secrets.redact_bytes(&body));
}

#[test]
fn a_byte_order_mark_and_a_control_character_in_a_string_make_a_body_that_is_not_json() {
    // Neither is JSON to the reader that is asked, so each body is searched as bytes:
    // the secret that stands across two strings is found, as no reading as JSON finds
    // it (`what_redaction_of_a_json_body_does_not_find_is_known`).
    let secrets = secrets(&["abcd\",\"efgh"]);
    let body = "[\"abcd\",\"efgh\"]";
    assert_eq!(secrets.redact_json(body.as_bytes()), body.as_bytes());
    for not_json in [
        format!("\u{feff}{body}"),
        format!("[\"\u{1}\",{}", &body[1..]),
        format!("[\"a\tb\",{}", &body[1..]),
    ] {
        let stored = secrets.redact_json(not_json.as_bytes());
        assert_eq!(stored, secrets.redact_bytes(not_json.as_bytes()));
        assert!(!holds(&stored, b"efgh"), "{not_json:?}");
    }
}

#[test]
fn a_body_nested_however_deep_is_read_as_json() {
    // A secret that stands across two strings is found by a search of bytes and not by
    // a reading as JSON (`what_redaction_of_a_json_body_does_not_find_is_known`): so
    // what comes back says which of the two a body got. At 200 levels and at 5,000 it
    // is read as JSON, and a key inside it is taken out of its string.
    let key = made_up_key("");
    let secrets = Secrets::new(["abcd\",\"efgh".to_owned(), key.clone()]).unwrap();
    for depth in [1, 127, 128, 129, 200, 5000] {
        let (open, close) = ("[".repeat(depth), "]".repeat(depth));
        let body = format!("{open}\"abcd\",\"efgh\",\"x {key}\"{close}");
        let stored = String::from_utf8(secrets.redact_json(body.as_bytes())).unwrap();
        assert_eq!(
            stored,
            format!("{open}\"abcd\",\"efgh\",\"x [redacted]\"{close}"),
            "{depth}"
        );
    }
}

#[test]
fn the_text_a_string_of_a_json_body_means_is_searched_with_the_readings_as_any_text_is() {
    // A backslash that is the body's own escape is decoded before the search. What the
    // string then means is a text like any other, and the readings are made of it.
    // A second encoding after a lone backslash of the text is as ADR 0014 accepts it:
    // in mixed escapes, beginning with a letter an escape ends in, it is missed.
    let letter = secrets(&["nabcdefgh12"]);
    let mixed = format!("\\n{}", u_all("abcdefgh12"));
    let found = serde_json::json!({ "t": &mixed[1..] }).to_string();
    let stored = String::from_utf8(letter.redact_json(found.as_bytes())).unwrap();
    assert_eq!(stored, "{\"t\":\"[redacted]\"}");
    let missed = serde_json::json!({ "t": mixed }).to_string();
    assert_eq!(letter.redact_json(missed.as_bytes()), missed.as_bytes());
    // A backslash of the text straight after a secret that ends in one goes with it:
    // the two are the secret as a JSON string writes it.
    let ends = secrets(&["abcdefgh\\"]);
    let body = serde_json::json!({ "t": "x abcdefgh\\\\ y", "u": "x abcdefgh\\ y" }).to_string();
    let stored = String::from_utf8(ends.redact_json(body.as_bytes())).unwrap();
    assert_eq!(
        json(&stored),
        serde_json::json!({ "t": "x [redacted] y", "u": "x [redacted] y" })
    );
}

#[test]
fn two_keys_that_each_held_a_secret_can_come_back_as_the_same_key() {
    let (one, two) = (made_up_key("-1"), made_up_key("-2"));
    let secrets = secrets(&[&one, &two]);
    let body = format!("{{\"{one}\":1,\"{two}\":2,\"k {one}\":3}}");
    let stored = String::from_utf8(secrets.redact_json(body.as_bytes())).unwrap();
    assert_eq!(
        stored,
        "{\"[redacted]\":1,\"[redacted]\":2,\"k [redacted]\":3}"
    );
    // Still JSON, to a reader that takes a key given twice.
    json(&stored);
}

#[test]
fn a_mebibyte_of_tiny_strings_against_one_long_value_is_redacted_inside_a_stated_time() {
    // A string of a few letters is searched in the room of a few letters, however long
    // the longest secret is (the room itself is held by a test beside the code). The
    // value is of marks, so its longest form is three times its length; each string
    // holds every mark a reading is made for, so each is followed twelve times. The
    // bound is loose, as the others are; what it took on a quiet machine, before and
    // after, is in ADR 0014.
    let long: String = (0..4096)
        .map(|n| char::from(b"!@#$^&*(){}[]<>?"[n % 15 + n / 1024 % 2]))
        .collect();
    let secrets = Secrets::new([long.clone()]).unwrap();
    let mut body = String::from("[");
    while body.len() < 1024 * 1024 {
        body.push_str("\"\\\\%+\\n\",");
    }
    body.push_str(&format!("\"{long}\"]"));
    let started = std::time::Instant::now();
    let stored = String::from_utf8(secrets.redact_json(body.as_bytes())).unwrap();
    let took = started.elapsed();
    assert_eq!(stored.len(), body.len() - long.len() + REDACTED.len());
    assert!(stored.ends_with("\\n\",\"[redacted]\"]"));
    assert!(took < std::time::Duration::from_secs(10), "{took:?}");
}
