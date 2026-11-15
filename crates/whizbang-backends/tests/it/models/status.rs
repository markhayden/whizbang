//! The status record an installer publishes as it goes (ADR 0017, "The status
//! record"): read strictly, bounded, and in the one form whizbang writes.

use whizbang_backends::models::{
    FILES_LIMIT, Refused, Rule, STATUS_FORMAT, STATUS_MAX_BYTES, State, Status, What,
};

use super::{MARKER, none, secrets, shows_nothing_of};

const STATES: [(State, &str); 4] = [
    (State::Installing, "installing"),
    (State::Installed, "installed"),
    (State::Failed, "failed"),
    (State::Cancelled, "cancelled"),
];

fn written(state: &str, done: &str, files: &str) -> Vec<u8> {
    format!(r#"{{"format":"status/1","state":"{state}","files_done":{done},"files":{files}}}"#)
        .into_bytes()
}

fn at(place: &str, rule: Rule) -> Refused {
    Refused::Value {
        at: place.to_owned(),
        rule,
    }
}

#[test]
fn a_status_record_round_trips_in_one_form() {
    assert_eq!(STATUS_FORMAT, "status/1");
    for (state, word) in STATES {
        let status = Status::new(state, 1, 2).unwrap();
        assert_eq!(
            (status.state(), status.files_done(), status.files()),
            (state, 1, 2)
        );
        let bytes = status.to_bytes();
        assert_eq!(bytes, written(word, "1", "2"));
        assert_eq!(Status::parse(&bytes, &none()).unwrap(), status);
    }
    // Bytes that say the same in another way are refused: white space, the fields
    // in another order, a character spelled as an escape, a line feed at the end.
    for other in [
        r#"{"format":"status/1","state":"failed","files_done":1,"files":2} "#,
        r#"{"format": "status/1","state":"failed","files_done":1,"files":2}"#,
        r#"{"state":"failed","format":"status/1","files_done":1,"files":2}"#,
        r#"{"format":"status/1","state":"fa\u0069led","files_done":1,"files":2}"#,
        "{\"format\":\"status/1\",\"state\":\"failed\",\"files_done\":1,\"files\":2}\n",
    ] {
        assert_eq!(
            Status::parse(other.as_bytes(), &none()).unwrap_err(),
            Refused::OtherForm { what: What::Status },
            "{other}"
        );
    }
}

#[test]
fn a_status_counts_one_to_a_bounded_count_of_files_and_no_more_staged_than_there_are() {
    let most = FILES_LIMIT as u64;
    for (done, files) in [(0, 1), (1, 1), (0, most), (most, most)] {
        let status = Status::new(State::Installing, done, files).unwrap();
        assert_eq!(Status::parse(&status.to_bytes(), &none()).unwrap(), status);
    }
    let refused = [
        ((0, 0), at("files", Rule::NoFiles)),
        (
            (0, most + 1),
            at("files", Rule::Count { most: FILES_LIMIT }),
        ),
        ((2, 1), at("files_done", Rule::Progress)),
        ((most + 1, most), at("files_done", Rule::Progress)),
    ];
    for ((done, files), refusal) in refused {
        assert_eq!(
            Status::new(State::Installing, done, files).unwrap_err(),
            refusal,
            "{done} of {files}"
        );
        let bytes = written("installing", &done.to_string(), &files.to_string());
        assert_eq!(
            Status::parse(&bytes, &none()).unwrap_err(),
            refusal,
            "{done} of {files}"
        );
    }
    // The longest record whizbang writes is far under the bound it reads back.
    let longest = Status::new(State::Installing, most, most)
        .unwrap()
        .to_bytes();
    assert!(longest.len() < 80, "{}", longest.len());
    assert_eq!(STATUS_MAX_BYTES, 256);
}

#[test]
fn a_status_record_is_read_strictly_and_refused_over_its_bound() {
    // Over the bound: refused for its length, before anything of it is looked at.
    // These bytes are not JSON and hold a secret, and neither is what is said.
    let mut long = MARKER.as_bytes().to_vec();
    long.resize(STATUS_MAX_BYTES as usize + 1, b'x');
    assert_eq!(
        Status::parse(&long, &secrets()).unwrap_err(),
        Refused::TooLarge {
            what: What::Status,
            limit: STATUS_MAX_BYTES
        }
    );
    long.truncate(STATUS_MAX_BYTES as usize);
    assert_eq!(
        Status::parse(&long, &secrets()).unwrap_err(),
        Refused::HoldsSecret
    );
    let refused: [(&str, Refused); 11] = [
        (
            "not json",
            Refused::NotJson {
                what: What::Status,
                line: 1,
            },
        ),
        ("[]", at("", Rule::Kind)),
        (
            r#"{"format":"status/2","state":"failed","files_done":1,"files":2}"#,
            at("format", Rule::Format),
        ),
        (
            r#"{"format":"status/1","state":"failed","files_done":1,"files":2,"more":1}"#,
            at("", Rule::Unknown),
        ),
        (
            r#"{"format":"status/1","state":"failed","files":2}"#,
            at("files_done", Rule::Missing),
        ),
        (
            r#"{"format":"status/1","files_done":1,"files":2}"#,
            at("state", Rule::Missing),
        ),
        (
            r#"{"format":"status/1","state":"failed","files_done":1}"#,
            at("files", Rule::Missing),
        ),
        (
            r#"{"format":"status/1","state":"Failed","files_done":1,"files":2}"#,
            at("state", Rule::State),
        ),
        (
            r#"{"format":"status/1","state":"removed","files_done":1,"files":2}"#,
            at("state", Rule::State),
        ),
        (
            r#"{"format":"status/1","state":"failed","files_done":"1","files":2}"#,
            at("files_done", Rule::Kind),
        ),
        (
            r#"{"format":"status/1","state":"failed","files_done":1,"files":2.0}"#,
            at("files", Rule::Kind),
        ),
    ];
    for (bytes, refusal) in refused {
        let got = Status::parse(bytes.as_bytes(), &none()).unwrap_err();
        assert_eq!(got, refusal, "{bytes}");
        shows_nothing_of(&got, &["removed", "more"]);
    }
    assert_eq!(
        Status::parse(
            br#"{"format":"status/1","state":"failed","state":"failed","files_done":1,"files":2}"#,
            &none()
        )
        .unwrap_err(),
        Refused::Twice {
            what: What::Status,
            line: 1
        }
    );
    // A secret anywhere in the bytes, a state's word among them, is refused whole.
    let told = whizbang_core::contract::Secrets::new(["installing".to_owned()]).unwrap();
    assert_eq!(
        Status::parse(&written("installing", "1", "2"), &told).unwrap_err(),
        Refused::HoldsSecret
    );
}
