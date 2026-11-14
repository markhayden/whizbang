//! What a refusal says (ADR 0017, "What a refusal says"): fixed words, a place in a
//! record and numbers. Every sentence there is, is here.

use whizbang_backends::models::{Refused, Rule, What};
use whizbang_core::egress::NotAllowed;

use super::shows_nothing_of;

#[test]
fn every_refusal_says_one_sentence_of_fixed_words() {
    let io = std::io::ErrorKind::NotFound;
    let said = |refused: Refused| {
        shows_nothing_of(&refused, &[]);
        refused.to_string()
    };
    for (what, name) in [
        (What::Catalogue, "the catalogue"),
        (What::Installation, "the installation record"),
    ] {
        assert_eq!(what.to_string(), name);
        assert_eq!(
            said(Refused::Unreadable { what, kind: io }),
            format!("{name} cannot be read: it is not there")
        );
        assert_eq!(
            said(Refused::Unreadable {
                what,
                kind: std::io::ErrorKind::PermissionDenied
            }),
            format!("{name} cannot be read: permission is denied")
        );
        assert_eq!(
            said(Refused::Unreadable {
                what,
                kind: std::io::ErrorKind::Other
            }),
            format!("{name} cannot be read: the system could not read it")
        );
        assert_eq!(
            said(Refused::TooLarge { what, limit: 77 }),
            format!("{name} is longer than 77 bytes")
        );
        assert_eq!(
            said(Refused::NotJson { what, line: 4 }),
            format!("{name} is not JSON at line 4")
        );
        assert_eq!(
            said(Refused::Twice { what, line: 9 }),
            format!("{name} gives one field twice, the second time at line 9")
        );
        assert_eq!(
            said(Refused::OtherForm { what }),
            format!("{name} is not in the one form it is written in")
        );
    }
    assert_eq!(
        said(Refused::HoldsSecret),
        "the text holds a secret, and is not shown"
    );
    let rules = [
        (Rule::Unknown, "holds a field that is not one of its fields"),
        (Rule::Missing, "is missing"),
        (Rule::Kind, "is of another kind than its field"),
        (Rule::Format, "is not the format this reader reads"),
        (
            Rule::ModelName,
            "is not a model's name: a lower-case letter or a digit, then lower-case letters, \
             digits, underscores, dots and hyphens, 64 characters at most",
        ),
        (
            Rule::FileName,
            "is not one plain file name: a letter or a digit, then letters, digits, \
             underscores, dots, hyphens and plus signs, 128 characters at most",
        ),
        (
            Rule::Revision,
            "is not a revision: a letter or a digit, then letters, digits, underscores, dots \
             and hyphens, 64 characters at most",
        ),
        (
            Rule::Licence,
            "is not a licence's name: 1 to 200 printable ASCII characters and spaces, with no \
             space at either end",
        ),
        (
            Rule::Feature,
            "is not a feature's name: a lower-case letter, then lower-case letters, digits, \
             underscores and hyphens, 64 characters at most",
        ),
        (
            Rule::Hash,
            "is not sha256, a colon and 64 lower-case hex digits",
        ),
        (
            Rule::Size,
            "is not a size from 1 byte to 1099511627776 bytes",
        ),
        (Rule::Count { most: 64 }, "holds more than 64"),
        (Rule::NoFiles, "holds no file, and a model has one at least"),
        (
            Rule::NamedTwice,
            "names one thing twice (two files are one when only the case of their letters \
             differs)",
        ),
        (
            Rule::Origin(NotAllowed::Port),
            "is not an origin: the port is not a number from 1 to 65535 written plainly",
        ),
        (
            Rule::OriginAlone,
            "is an origin with a path after it, and an origin has none",
        ),
        (
            Rule::Address(NotAllowed::Fragment),
            "is not an address: the address has a fragment, and may have none",
        ),
        (Rule::Elsewhere, "is at an origin its model does not name"),
    ];
    for (rule, sentence) in rules {
        assert_eq!(rule.to_string(), sentence);
        // A value in a record is named by its place; a value by itself is not.
        assert_eq!(
            said(Refused::Value {
                at: "models[2].files[0].size".to_owned(),
                rule
            }),
            format!("`models[2].files[0].size` {sentence}")
        );
        assert_eq!(
            said(Refused::Value {
                at: String::new(),
                rule
            }),
            format!("the value {sentence}")
        );
    }
}
