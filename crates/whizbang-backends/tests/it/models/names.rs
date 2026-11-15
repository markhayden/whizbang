//! The texts that come from outside the build and become names on a disk or words in a
//! record (ADR 0017, "Names"): a model's name, a file's name, a revision and a licence.

use whizbang_backends::models::{
    FILE_NAME_LIMIT, FileName, LICENCE_LIMIT, Licence, MODEL_NAME_LIMIT, ModelName, REVISION_LIMIT,
    Refused, Revision, Rule, SHOWN_LIMIT,
};

use super::{MARKER, none, secrets, shows_nothing_of};

fn refused_as(refused: Refused, rule: Rule) {
    assert_eq!(
        refused,
        Refused::Value {
            at: String::new(),
            rule
        }
    );
}

#[test]
fn a_models_name_is_lower_case_letters_digits_and_three_marks_and_starts_with_none_of_the_marks() {
    for name in ["sam3", "tiny", "3d", "sam2.1-hiera_tiny", "a", "0"] {
        assert_eq!(ModelName::new(name, &none()).unwrap().as_str(), name);
    }
    let longest = "m".repeat(MODEL_NAME_LIMIT);
    assert_eq!(MODEL_NAME_LIMIT, 64);
    assert!(ModelName::new(&longest, &none()).is_ok());
    let too_long = "m".repeat(MODEL_NAME_LIMIT + 1);
    for name in [
        "",
        ".",
        "..",
        ".hidden",
        "-dash",
        "_under",
        "Sam3",
        "sam3/x",
        "sam3\\x",
        "sam 3",
        "sam3\n",
        "sam3\0",
        "sam+3",
        "s\u{e9}m",
        "/sam3",
        "models/../sam3",
        too_long.as_str(),
    ] {
        let refused = ModelName::new(name, &none()).unwrap_err();
        refused_as(refused.clone(), Rule::ModelName);
        if name.len() > 3 {
            shows_nothing_of(&refused, &[name]);
        }
    }
}

#[test]
fn a_files_name_is_one_plain_name() {
    for name in [
        "graph.onnx",
        "LICENSE.sam3.txt",
        "sam2.1-hiera-tiny.encoder.onnx",
        "evf-sam2.prompted.0.data",
        "a",
        "7z",
        "weights+lora_v2.bin",
    ] {
        assert_eq!(FileName::new(name, &none()).unwrap().as_str(), name);
    }
    assert_eq!(FILE_NAME_LIMIT, 128);
    assert!(FileName::new(&"f".repeat(FILE_NAME_LIMIT), &none()).is_ok());
    let too_long = "f".repeat(FILE_NAME_LIMIT + 1);
    for name in [
        "",
        ".",
        "..",
        ".hidden",
        "-rf",
        "_x",
        "+x",
        "a/b",
        "a\\b",
        "/etc/passwd",
        "../up",
        "a b",
        "a\tb",
        "a\nb",
        "a\0b",
        "a:b",
        "a*b",
        "a\"b",
        "na\u{ef}ve.onnx",
        too_long.as_str(),
    ] {
        let refused = FileName::new(name, &none()).unwrap_err();
        refused_as(refused.clone(), Rule::FileName);
        if name.len() > 3 {
            shows_nothing_of(&refused, &[name]);
        }
    }
}

#[test]
fn a_revision_is_letters_digits_and_three_marks() {
    for text in [
        "1",
        "2026-11.1",
        "v2_rc1",
        "2a1560802f8cf3c408fec9b809d705f56a2f7146",
        "A",
    ] {
        assert_eq!(Revision::new(text, &none()).unwrap().as_str(), text);
    }
    assert_eq!(REVISION_LIMIT, 64);
    assert!(Revision::new(&"7".repeat(REVISION_LIMIT), &none()).is_ok());
    let too_long = "7".repeat(REVISION_LIMIT + 1);
    for text in [
        "",
        ".1",
        "-1",
        "_1",
        "1 2",
        "1/2",
        "1\n",
        "r\u{e9}v",
        too_long.as_str(),
    ] {
        refused_as(Revision::new(text, &none()).unwrap_err(), Rule::Revision);
    }
}

#[test]
fn a_licence_is_one_line_of_printable_ascii() {
    for text in [
        "MIT",
        "Apache-2.0",
        "SAM License",
        "MIT and Apache-2.0",
        "x",
    ] {
        assert_eq!(Licence::new(text, &none()).unwrap().as_str(), text);
    }
    assert_eq!(LICENCE_LIMIT, 200);
    assert!(Licence::new(&"l".repeat(LICENCE_LIMIT), &none()).is_ok());
    let too_long = "l".repeat(LICENCE_LIMIT + 1);
    for text in [
        "",
        " MIT",
        "MIT ",
        " ",
        "MIT\nApache",
        "MIT\tApache",
        "licen\u{e7}e",
        "a\0b",
        too_long.as_str(),
    ] {
        refused_as(Licence::new(text, &none()).unwrap_err(), Rule::Licence);
    }
}

#[test]
fn a_name_that_holds_a_secret_is_refused_whole_before_its_rule_and_not_shown() {
    let inside = format!("tiny-{MARKER}");
    // A name that keeps its rule but for the secret, and one that breaks its rule too:
    // each is refused for the secret, whatever else is wrong with it.
    let broken = format!("../{MARKER} and more");
    for text in [MARKER, inside.as_str(), broken.as_str()] {
        let refusals = [
            ModelName::new(text, &secrets()).unwrap_err(),
            FileName::new(text, &secrets()).unwrap_err(),
            Revision::new(text, &secrets()).unwrap_err(),
            Licence::new(text, &secrets()).unwrap_err(),
        ];
        for refused in refusals {
            assert_eq!(refused, Refused::HoldsSecret);
            shows_nothing_of(&refused, &[MARKER, &MARKER[..4]]);
        }
    }
    // Told of no secret, the same names are names.
    assert!(ModelName::new(MARKER, &none()).is_ok());
    assert!(FileName::new(&inside, &none()).is_ok());
    assert!(Revision::new(MARKER, &none()).is_ok());
    assert!(Licence::new(&inside, &none()).is_ok());
}

#[test]
fn what_is_shown_of_a_name_is_its_first_characters_and_three_dots_when_there_are_more() {
    assert_eq!(SHOWN_LIMIT, 64);
    let short = FileName::new("graph.onnx", &none()).unwrap();
    assert_eq!(short.shown(), "graph.onnx");
    let exactly = FileName::new(&"e".repeat(SHOWN_LIMIT), &none()).unwrap();
    assert_eq!(exactly.shown(), "e".repeat(SHOWN_LIMIT));
    let long = FileName::new(&format!("{}tail", "h".repeat(SHOWN_LIMIT)), &none()).unwrap();
    assert_eq!(long.shown(), format!("{}...", "h".repeat(SHOWN_LIMIT)));
    // A model's name is never longer than what is shown.
    let longest = ModelName::new(&"m".repeat(MODEL_NAME_LIMIT), &none()).unwrap();
    assert_eq!(longest.shown(), longest.as_str());
    // What `{:?}` prints of a name is what is shown of it.
    assert_eq!(format!("{long:?}"), format!("FileName({})", long.shown()));
    assert_eq!(
        format!("{longest:?}"),
        format!("ModelName({})", longest.as_str())
    );
}
