//! The rule for a name and a purpose, held in Rust as `scripts/docs.sh` holds it. The
//! cases are those of `scripts/tests/test-docs.sh`, so the two cannot drift unseen.

use whizbang_core::contract::text;

#[test]
fn an_action_name_is_a_lower_case_letter_then_letters_digits_underscores_or_dashes() {
    for name in ["a", "grow", "zebra_map", "apple-2", "remove_prompt"] {
        text::action_name(name).unwrap_or_else(|why| panic!("{name:?} was refused: {why}"));
    }
    text::action_name(&"x".repeat(64)).unwrap();
}

#[test]
fn every_action_name_the_reference_refuses_is_refused() {
    let long = "x".repeat(65);
    for name in [
        "",
        "Zebra",
        "9lives",
        "-dash",
        "_under",
        "two words",
        "tick`",
        "a.b",
        "a|b",
        "new\nline",
        "café",
        long.as_str(),
    ] {
        assert!(text::action_name(name).is_err(), "{name:?} was accepted");
    }
}

#[test]
fn a_field_name_is_an_action_name_without_dashes() {
    for name in ["px", "down_px", "image", "a1"] {
        text::field_name(name).unwrap_or_else(|why| panic!("{name:?} was refused: {why}"));
    }
    let long = "x".repeat(65);
    for name in ["", "down-px", "Px", "1px", "_px", "p x", long.as_str()] {
        assert!(text::field_name(name).is_err(), "{name:?} was accepted");
    }
}

#[test]
fn a_purpose_is_one_plain_line() {
    for purpose in [
        "Listed first: 2 images in, 1 map out (50% faster).",
        "Listed second.",
        "A",
        "It's \"quoted\"; a/b + c - d, e: f.",
    ] {
        text::purpose(purpose).unwrap_or_else(|why| panic!("{purpose:?} was refused: {why}"));
    }
    text::purpose(&"A".repeat(200)).unwrap();
}

#[test]
fn every_purpose_the_reference_refuses_is_refused() {
    let long = "A".repeat(201);
    for purpose in [
        "",
        " Leading space.",
        "Trailing space. ",
        "# A heading",
        "2. A list item",
        "- A list item",
        "Two\nlines.",
        "Text\n\n## `forged`\n\nA forged action.",
        "A carriage\rreturn.",
        "A\ttab.",
        "A bell\u{7}.",
        "A <b>tag</b>.",
        "An <!-- unclosed comment.",
        "A quote > mark.",
        "A `tick`.",
        "A | bar.",
        "A [link](https://example.com).",
        "An *emphasis*.",
        "An _emphasis_.",
        "A back\\slash.",
        "See https://example.com/page for more.",
        "See www.example.com for more.",
        "See WWW.example.com for more.",
        "See example.com://x.",
        "An &amp; entity.",
        "A line separator\u{2028}here.",
        "Café.",
        long.as_str(),
    ] {
        assert!(text::purpose(purpose).is_err(), "{purpose:?} was accepted");
    }
}

#[test]
fn a_version_is_three_numbers() {
    for version in ["0.0.1", "1.2.3", "10.20.30"] {
        text::version(version).unwrap_or_else(|why| panic!("{version:?} was refused: {why}"));
    }
    for version in [
        "",
        "1.2",
        "1..3",
        "1.2.3 beta",
        "1.2.3\n",
        "a.b.c",
        "1.2.3.4",
        "+1.2.3",
    ] {
        assert!(text::version(version).is_err(), "{version:?} was accepted");
    }
}
