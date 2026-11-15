//! The status record the holder of a model's lock publishes as it goes (ADR 0017,
//! "The status record"): read strictly, bounded, and in the one form whizbang writes.

use whizbang_backends::models::{
    FILES_LIMIT, InstallationId, Refused, Rule, STATUS_FORMAT, STATUS_MAX_BYTES, State, Status,
    What,
};

use super::publishing::{record_of, second};
use super::{MARKER, none, secrets, shows_nothing_of};

/// The states of an install, which name an installation and count files.
const STATES: [(State, &str); 4] = [
    (State::Installing, "installing"),
    (State::Installed, "installed"),
    (State::Failed, "failed"),
    (State::Cancelled, "cancelled"),
];

/// The states of a removal, which do neither.
const REMOVALS: [(State, &str); 2] = [(State::Removing, "removing"), (State::Removed, "removed")];

/// The identity every status of an install here names.
fn id() -> InstallationId {
    record_of(&second()).identity()
}

fn written(state: &str, done: &str, files: &str) -> Vec<u8> {
    let id = id();
    format!(
        r#"{{"format":"status/2","state":"{state}","installation":"{id}","files_done":{done},"files":{files}}}"#
    )
    .into_bytes()
}

/// A record of an install as it is written, with `this` put in place of `that`.
fn with(that: &str, this: &str) -> String {
    let whole = String::from_utf8(written("failed", "1", "2")).unwrap();
    assert!(whole.contains(that), "{that}");
    whole.replacen(that, this, 1)
}

fn at(place: &str, rule: Rule) -> Refused {
    Refused::Value {
        at: place.to_owned(),
        rule,
    }
}

#[test]
fn a_status_record_round_trips_in_one_form() {
    assert_eq!(STATUS_FORMAT, "status/2");
    for (state, word) in STATES {
        let status = Status::of_install(state, id(), 1, 2).unwrap();
        assert_eq!(
            (status.state(), status.files_done(), status.files()),
            (state, 1, 2)
        );
        assert_eq!(status.installation(), Some(id()));
        assert!(!state.is_of_a_removal());
        let bytes = status.to_bytes();
        assert_eq!(bytes, written(word, "1", "2"));
        assert_eq!(Status::parse(&bytes, &none()).unwrap(), status);
    }
    // A removal's record is its format and its state, and nothing else.
    for (state, word) in REMOVALS {
        let status = Status::of_removal(state).unwrap();
        assert_eq!(
            (status.state(), status.files_done(), status.files()),
            (state, 0, 0)
        );
        assert_eq!(status.installation(), None);
        assert!(state.is_of_a_removal());
        let bytes = status.to_bytes();
        assert_eq!(
            bytes,
            format!(r#"{{"format":"status/2","state":"{word}"}}"#).into_bytes()
        );
        assert_eq!(Status::parse(&bytes, &none()).unwrap(), status);
    }
    // Bytes that say the same in another way are refused: white space, the fields
    // in another order, a character spelled as an escape, a line feed at the end.
    let whole = with("failed", "failed");
    for other in [
        format!("{whole} "),
        with(r#""format":"#, r#""format": "#),
        with(
            r#""format":"status/2","state":"failed""#,
            r#""state":"failed","format":"status/2""#,
        ),
        with("failed", "fa\\u0069led"),
        format!("{whole}\n"),
        r#"{"format":"status/2","state":"removed"} "#.to_owned(),
        r#"{"state":"removing","format":"status/2"}"#.to_owned(),
    ] {
        assert_eq!(
            Status::parse(other.as_bytes(), &none()).unwrap_err(),
            Refused::OtherForm { what: What::Status },
            "{other}"
        );
    }
}

#[test]
fn a_status_of_an_install_names_its_installation_and_one_of_a_removal_names_none() {
    // The two constructors take the states of their own kind and no other.
    for (state, _) in REMOVALS {
        assert_eq!(
            Status::of_install(state, id(), 1, 2).unwrap_err(),
            at("state", Rule::State)
        );
    }
    for (state, _) in STATES {
        assert_eq!(
            Status::of_removal(state).unwrap_err(),
            at("state", Rule::State)
        );
    }
    // Read back: a removal with a field of an install, and an install without its
    // installation, are refused; so is an identity that is not one as it is written.
    let id = id();
    let refused = [
        (
            format!(r#"{{"format":"status/2","state":"removing","installation":"{id}"}}"#),
            at("", Rule::Unknown),
        ),
        (
            r#"{"format":"status/2","state":"removed","files_done":0,"files":1}"#.to_owned(),
            at("", Rule::Unknown),
        ),
        (
            r#"{"format":"status/2","state":"failed","files_done":1,"files":2}"#.to_owned(),
            at("installation", Rule::Missing),
        ),
        (
            with(&id.to_string(), &id.hex()),
            at("installation", Rule::Hash),
        ),
        (
            with(&id.to_string(), &id.to_string().to_uppercase()),
            at("installation", Rule::Hash),
        ),
        (
            with(&format!("\"{id}\""), "7"),
            at("installation", Rule::Kind),
        ),
    ];
    for (bytes, refusal) in refused {
        let got = Status::parse(bytes.as_bytes(), &none()).unwrap_err();
        assert_eq!(got, refusal, "{bytes}");
        shows_nothing_of(&got, &[&id.hex()]);
    }
    // An identity that holds a secret the reader was told of is refused whole.
    let told = whizbang_core::contract::Secrets::new([id.hex()[3..40].to_owned()]).unwrap();
    assert_eq!(
        Status::parse(&written("installed", "2", "2"), &told).unwrap_err(),
        Refused::HoldsSecret
    );
}

#[test]
fn a_status_counts_one_to_a_bounded_count_of_files_and_no_more_staged_than_there_are() {
    let most = FILES_LIMIT as u64;
    for (done, files) in [(0, 1), (1, 1), (0, most), (most, most)] {
        let status = Status::of_install(State::Installing, id(), done, files).unwrap();
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
            Status::of_install(State::Installing, id(), done, files).unwrap_err(),
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
    let longest = Status::of_install(State::Installing, id(), most, most)
        .unwrap()
        .to_bytes();
    assert!(longest.len() < 160, "{}", longest.len());
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
    let refused: [(String, Refused); 12] = [
        (
            "not json".to_owned(),
            Refused::NotJson {
                what: What::Status,
                line: 1,
            },
        ),
        ("[]".to_owned(), at("", Rule::Kind)),
        // The format before this one, and one after it.
        (with("status/2", "status/1"), at("format", Rule::Format)),
        (with("status/2", "status/3"), at("format", Rule::Format)),
        (
            with(r#""files":2"#, r#""files":2,"more":1"#),
            at("", Rule::Unknown),
        ),
        (
            with(r#""files_done":1,"#, ""),
            at("files_done", Rule::Missing),
        ),
        (with(r#""state":"failed","#, ""), at("state", Rule::Missing)),
        (with(r#","files":2"#, ""), at("files", Rule::Missing)),
        (with("failed", "Failed"), at("state", Rule::State)),
        (with("failed", "gone"), at("state", Rule::State)),
        (
            with(r#""files_done":1"#, r#""files_done":"1""#),
            at("files_done", Rule::Kind),
        ),
        (
            with(r#""files":2"#, r#""files":2.0"#),
            at("files", Rule::Kind),
        ),
    ];
    for (bytes, refusal) in refused {
        let got = Status::parse(bytes.as_bytes(), &none()).unwrap_err();
        assert_eq!(got, refusal, "{bytes}");
        shows_nothing_of(&got, &["gone", "more"]);
    }
    assert_eq!(
        Status::parse(
            with(
                r#""state":"failed""#,
                r#""state":"failed","state":"failed""#
            )
            .as_bytes(),
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
