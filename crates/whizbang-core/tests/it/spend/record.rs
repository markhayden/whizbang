//! The content under each of the ledger's names (ADR 0016, "The content of each
//! record").

use whizbang_core::capability::Stage;
use whizbang_core::spend::{
    ClosedBy, Name, PlaceBy, Record, SPEND_RECORD_MAX_BYTES, Seal, SpendRecordError, Sum, TurnBy,
};

use super::{amount, n, run};

/// One record of each kind, with the one line it is written as.
fn every_kind() -> Vec<(Record, String)> {
    let seal = Seal::parse("00112233445566778899aabbccddeeff").unwrap();
    let line = |text: &str| format!("{text}\n");
    vec![
        (
            Record::Hold {
                epoch: 1,
                run: run("7-1"),
                reserved: amount(10),
            },
            line(r#"{"record":"spend-hold/1","epoch":1,"run":"7-1","reserved_micros":10}"#),
        ),
        (
            Record::Turn {
                epoch: 1,
                turn: n(2),
                by: TurnBy::Admission,
                run: Some(run("7-1")),
            },
            line(r#"{"record":"spend-turn/1","epoch":1,"turn":2,"by":"admission","run":"7-1"}"#),
        ),
        (
            Record::Turn {
                epoch: 1,
                turn: n(3),
                by: TurnBy::Reset,
                run: None,
            },
            line(r#"{"record":"spend-turn/1","epoch":1,"turn":3,"by":"reset","run":null}"#),
        ),
        (
            Record::Reset {
                epoch: n(2),
                allowance: amount(20),
                at_ms: 7,
            },
            line(r#"{"record":"spend-reset/1","epoch":2,"allowance_micros":20,"at_ms":7}"#),
        ),
        (
            Record::Epoch { epoch: n(2) },
            line(r#"{"record":"spend-epoch/1","epoch":2}"#),
        ),
        (
            Record::Place {
                run: run("7-1"),
                place: n(1),
                by: PlaceBy::Dispatch,
            },
            line(r#"{"record":"spend-place/1","run":"7-1","place":1,"by":"dispatch"}"#),
        ),
        (
            Record::Dispatch {
                run: run("7-1"),
                place: n(1),
                by: run("7-2"),
                request: 0,
                bound: amount(5),
                at_ms: 8,
            },
            line(
                r#"{"record":"spend-dispatch/1","run":"7-1","place":1,"by":"7-2","request":0,"bound_micros":5,"at_ms":8}"#,
            ),
        ),
        (
            Record::Settling {
                run: run("7-1"),
                place: n(1),
            },
            line(r#"{"record":"spend-settling/1","run":"7-1","place":1}"#),
        ),
        (
            Record::Settlement {
                run: run("7-1"),
                place: n(1),
                stage: Stage::Answered,
                charged: amount(12),
                over_bound: true,
                at_ms: 9,
            },
            line(
                r#"{"record":"spend-settlement/1","run":"7-1","place":1,"stage":"answered","charged_micros":12,"over_bound":true,"at_ms":9}"#,
            ),
        ),
        (
            Record::Closing {
                run: run("7-1"),
                released: false,
                by: ClosedBy::Recovery,
                charged: Sum::new(12),
                // A sum over settlements can pass 64 bits, and can have stopped.
                unresolved: Sum::LARGEST,
                at_ms: 10,
            },
            line(
                r#"{"record":"spend-closing/1","run":"7-1","released":false,"by":"recovery","charged_micros":"12","unresolved_micros":"340282366920938463463374607431768211455","at_ms":10}"#,
            ),
        ),
        (
            Record::Adjustment {
                run: run("7-1"),
                place: n(3),
                turn: n(4),
                seal,
                charged: amount(0),
                at_ms: 11,
            },
            line(
                r#"{"record":"spend-adjustment/1","run":"7-1","place":3,"turn":4,"seal":"00112233445566778899aabbccddeeff","charged_micros":0,"at_ms":11}"#,
            ),
        ),
    ]
}

#[test]
fn each_record_is_written_one_way_and_round_trips_to_the_same_bytes() {
    let all = every_kind();
    let kinds: std::collections::BTreeSet<&str> = all.iter().map(|(one, _)| one.kind()).collect();
    assert_eq!(kinds, Record::KINDS.into_iter().collect());
    for (record, line) in all {
        assert_eq!(String::from_utf8(record.to_bytes()).unwrap(), line);
        let read = Record::from_bytes(line.as_bytes()).unwrap();
        assert_eq!(read, record);
        assert_eq!(read.to_bytes(), line.as_bytes());
        // It repeats what its name says: under its name it is read, and under any
        // other it is damage.
        let name = record.name();
        assert_eq!(Record::read(&name, line.as_bytes()).unwrap(), record);
        let other = Name::Epoch(n(77));
        assert_eq!(
            Record::read(&other, line.as_bytes()),
            Err(SpendRecordError::Disagrees)
        );
        assert!(name.to_string().len() <= 200);
    }
}

#[test]
fn a_record_with_an_unknown_or_a_missing_field_or_any_other_bytes_is_refused() {
    for (record, line) in every_kind() {
        let kind = record.kind();
        let body = line.trim_end().strip_suffix('}').unwrap();
        // A field more.
        let more = format!("{body},\"more\":1}}\n");
        let refused = Record::from_bytes(more.as_bytes()).expect_err(kind);
        assert!(matches!(refused, SpendRecordError::Malformed(_)), "{kind}");
        assert!(refused.to_string().contains("unknown field"), "{refused}");
        // Each field left out in turn, the kind aside.
        let value: serde_json::Value = serde_json::from_str(&line).unwrap();
        let fields: Vec<String> = value.as_object().unwrap().keys().cloned().collect();
        for field in fields.iter().filter(|field| *field != "record") {
            let mut less = value.as_object().unwrap().clone();
            less.remove(field);
            let less = format!("{}\n", serde_json::to_string(&less).unwrap());
            let refused = Record::from_bytes(less.as_bytes()).expect_err(field);
            assert!(
                refused.to_string().contains("missing field"),
                "{kind} without {field}: {refused}"
            );
        }
        // The same record written another way: with a space, and without its newline.
        let spaced = line.replacen(':', ": ", 1);
        assert_eq!(
            Record::from_bytes(spaced.as_bytes()),
            Err(SpendRecordError::NotCanonical)
        );
        assert_eq!(
            Record::from_bytes(line.trim_end().as_bytes()),
            Err(SpendRecordError::NotCanonical)
        );
    }
}

#[test]
fn a_record_over_its_bound_or_of_another_kind_or_that_says_what_none_can_is_refused() {
    let long = format!(
        "{{\"record\":\"spend-epoch/1\",\"epoch\":2{}}}\n",
        " ".repeat(SPEND_RECORD_MAX_BYTES as usize)
    );
    assert_eq!(
        Record::from_bytes(long.as_bytes()),
        Err(SpendRecordError::TooLarge {
            bytes: long.len() as u64
        })
    );
    // Another kind: quoted escaped and cut, with no more of it than a line.
    let hostile = format!("spend-\n\u{1b}{}", "x".repeat(900));
    let other = format!(
        "{{\"record\":{}}}\n",
        serde_json::to_string(&hostile).unwrap()
    );
    let said = Record::from_bytes(other.as_bytes())
        .unwrap_err()
        .to_string();
    assert!(!said.contains('\n') && !said.contains('\u{1b}'), "{said}");
    assert!(said.contains("\\n") && said.len() < 200, "{said}");
    let not_json = b"\x00\xff not json\n";
    let said = Record::from_bytes(not_json).unwrap_err().to_string();
    assert!(said.starts_with("not a record of the ledger"), "{said}");
    // The largest number is no amount, in a record as in a name; a place is from 1.
    let inconsistent = [
        r#"{"record":"spend-hold/1","epoch":1,"run":"7-1","reserved_micros":18446744073709551615}"#,
        r#"{"record":"spend-reset/1","epoch":2,"allowance_micros":18446744073709551615,"at_ms":7}"#,
        r#"{"record":"spend-dispatch/1","run":"7-1","place":1,"by":"7-2","request":0,"bound_micros":18446744073709551615,"at_ms":8}"#,
        r#"{"record":"spend-settlement/1","run":"7-1","place":1,"stage":"answered","charged_micros":18446744073709551615,"over_bound":true,"at_ms":9}"#,
        r#"{"record":"spend-adjustment/1","run":"7-1","place":3,"turn":4,"seal":"00112233445566778899aabbccddeeff","charged_micros":18446744073709551615,"at_ms":11}"#,
        r#"{"record":"spend-place/1","run":"7-1","place":0,"by":"dispatch"}"#,
        r#"{"record":"spend-place/1","run":"7-1","place":1,"by":"nothing"}"#,
        r#"{"record":"spend-place/1","run":"NOT AN ID","place":1,"by":"dispatch"}"#,
        r#"{"record":"spend-epoch/1","epoch":0}"#,
        r#"{"record":"spend-settlement/1","run":"7-1","place":1,"stage":"sent","charged_micros":1,"over_bound":true,"at_ms":9}"#,
        r#"{"record":"spend-closing/1","run":"7-1","released":false,"by":"recovery","charged_micros":"012","unresolved_micros":"0","at_ms":10}"#,
        r#"{"record":"spend-closing/1","run":"7-1","released":false,"by":"recovery","charged_micros":"340282366920938463463374607431768211456","unresolved_micros":"0","at_ms":10}"#,
        r#"{"record":"spend-adjustment/1","run":"7-1","place":3,"turn":4,"seal":"0011","charged_micros":0,"at_ms":11}"#,
    ];
    for text in inconsistent {
        let refused = Record::from_bytes(format!("{text}\n").as_bytes()).expect_err(text);
        assert!(
            matches!(refused, SpendRecordError::Inconsistent(_)),
            "{text}: {refused:?}"
        );
    }
}
