//! The rule for a name and a purpose, held in Rust as `scripts/docs.sh` holds it. One table
//! of cases is run against both, here and in `scripts/tests/test-docs.sh`.

use whizbang_core::contract::text;

/// The case table, which `scripts/tests/test-docs.sh` runs against `scripts/docs.sh`.
const CASES: &str = include_str!("../../../../../scripts/tests/fixtures/names-and-purposes.tsv");

/// Every case of the table: whether it is accepted, whether it is a name (or a purpose),
/// and the string.
fn cases() -> Vec<(bool, bool, String)> {
    CASES
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            assert_eq!(fields.len(), 3, "{line:?}");
            let accept = match fields[0] {
                "accept" => true,
                "refuse" => false,
                other => panic!("neither accept nor refuse: {other:?}"),
            };
            let name = match fields[1] {
                "name" => true,
                "purpose" => false,
                other => panic!("neither a name nor a purpose: {other:?}"),
            };
            (accept, name, serde_json::from_str(fields[2]).unwrap())
        })
        .collect()
}

#[test]
fn every_case_of_the_table_the_reference_is_held_to_gets_the_same_answer_here() {
    let cases = cases();
    // The table as it was first written: one that was not read, or was cut short, fails.
    assert!(cases.len() >= 51, "{} cases", cases.len());
    for (accept, name, string) in &cases {
        let answer = if *name {
            text::action_name(string)
        } else {
            text::purpose(string)
        };
        assert_eq!(answer.is_ok(), *accept, "{string:?}: {answer:?}");
    }
    // Each of the four kinds of case is there.
    for kind in [(true, true), (true, false), (false, true), (false, false)] {
        assert!(
            cases
                .iter()
                .any(|(accept, name, _)| (*accept, *name) == kind)
        );
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
fn a_version_is_three_numbers() {
    for version in [
        "0.0.1",
        "1.2.3",
        "10.20.30",
        "0.0.0",
        "999999999.0.999999999",
    ] {
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
        // One way to write each number: no leading zero, and nine digits at most.
        "01.2.3",
        "1.02.3",
        "1.2.00",
        "1000000000.0.0",
    ] {
        assert!(text::version(version).is_err(), "{version:?} was accepted");
    }
}
