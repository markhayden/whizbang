//! A fault: a typed error of a capability, with one plain sentence that cannot hold a
//! path or an address (ADR 0006, "Faults").

use whizbang_core::capability::{FAULT_LIMIT, Fault, FaultKind, Unfit};

#[test]
fn every_kind_has_its_name() {
    let names: Vec<&str> = FaultKind::ALL.iter().map(|kind| kind.name()).collect();
    assert_eq!(
        names,
        [
            "cancelled",
            "unsupported",
            "credential",
            "rejected",
            "unavailable",
            "timed_out",
            "malformed",
            "too_large",
        ]
    );
}

#[test]
fn a_fault_says_its_kind_and_its_sentence() {
    let fault = Fault::new(FaultKind::Rejected, "status 400: the image is too small");
    assert_eq!(fault.kind(), FaultKind::Rejected);
    assert_eq!(fault.sentence(), "status 400: the image is too small");
    assert_eq!(
        fault.to_string(),
        "rejected: status 400: the image is too small"
    );
    assert_eq!(fault.wait_ms(), None);
    let later = Fault::new(FaultKind::Unavailable, "overloaded").retry_after_ms(1500);
    assert_eq!(later.wait_ms(), Some(1500));
}

#[test]
fn a_sentence_cannot_hold_a_path_an_address_or_a_line_break() {
    // What an adapter might pass on from a failed request: a place on the machine, an
    // address with a credential in it, a header. Every character that could spell a
    // path or an address is replaced, in the text and in what `{:?}` prints.
    let text = "open /srv/keys/key failed\nGET https://user:pw@host/v1?k=v \
                C:\\keys\\key \"x\" <b> key=abcdefgh";
    let fault = Fault::new(FaultKind::Unavailable, text);
    let expected = "open ?srv?keys?key failed?GET https:??user:pw?host?v1?k?v \
                    C:?keys?key ?x? ?b? key?abcdefgh";
    assert_eq!(fault.sentence(), expected);
    assert!(!fault.to_string().contains('"'));
    // `{:?}` puts its own quotes round the sentence, and nothing else.
    let debug = format!("{fault:?}");
    assert!(
        debug.contains(&format!("sentence: \"{expected}\"")),
        "{debug}"
    );
    for shown in [fault.to_string(), debug] {
        for refused in ['/', '\\', '\n', '@', '=', '<', '>'] {
            assert!(!shown.contains(refused), "{refused:?} in {shown}");
        }
    }
    // Every character that is kept, kept.
    let kept = "Az09 .,;:'()%+-_";
    assert_eq!(Fault::new(FaultKind::Malformed, kept).sentence(), kept);
    // Text that is not ASCII is replaced character by character.
    assert_eq!(
        Fault::new(FaultKind::Malformed, "caf\u{e9}").sentence(),
        "caf?"
    );
}

#[test]
fn a_sentence_is_cut_at_its_limit_and_is_never_empty() {
    assert_eq!(FAULT_LIMIT, 240);
    let long = "a".repeat(FAULT_LIMIT + 60);
    let fault = Fault::new(FaultKind::Malformed, &long);
    assert_eq!(fault.sentence().len(), FAULT_LIMIT);
    assert_eq!(fault.sentence(), "a".repeat(FAULT_LIMIT));
    // A long text of characters that take several bytes is cut by characters.
    let wide = "\u{1f600}".repeat(FAULT_LIMIT + 1);
    assert_eq!(
        Fault::new(FaultKind::Malformed, &wide).sentence(),
        "?".repeat(FAULT_LIMIT)
    );
    assert_eq!(
        Fault::new(FaultKind::Malformed, "").sentence(),
        "no reason was given"
    );
}

#[test]
fn something_unfit_is_a_malformed_answer() {
    let unfit = Unfit::WrongLength {
        what: "the fill",
        expected: 12,
        found: 16,
    };
    assert_eq!(unfit.to_string(), "the fill has 16 bytes, not 12");
    let fault = Fault::from(unfit);
    assert_eq!(fault.kind(), FaultKind::Malformed);
    assert_eq!(fault.sentence(), "the fill has 16 bytes, not 12");
}

#[test]
fn each_way_of_being_unfit_is_a_plain_sentence() {
    let cases = [
        (Unfit::Empty { what: "the fill" }, "the fill is empty"),
        (
            Unfit::TooLarge {
                what: "the image",
                size: 50_000_001,
                limit: 50_000_000,
            },
            "the image is 50000001, over the limit of 50000000",
        ),
        (
            Unfit::WrongSize {
                what: "the fill",
                expected: (4, 3),
                found: (3, 4),
            },
            "the fill is 3 by 4, not 4 by 3",
        ),
        (
            Unfit::OutOfRange {
                what: "the support plane",
                at: Some(5),
                found: "128".to_owned(),
                allowed: "0 or 255",
            },
            "the support plane holds 128 at 5: a value is 0 or 255",
        ),
        (
            Unfit::OutOfRange {
                what: "the score",
                at: None,
                found: "NaN".to_owned(),
                allowed: "from 0 to 1",
            },
            "the score is NaN: a value is from 0 to 1",
        ),
        (
            Unfit::NotAllowed {
                what: "a system message",
                why: "holds text only",
            },
            "a system message holds text only",
        ),
    ];
    for (unfit, sentence) in cases {
        assert_eq!(unfit.to_string(), sentence);
    }
}
