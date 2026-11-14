//! The installation record (ADR 0017, "An installation and its record"): what one
//! installation of a model holds, and the revisions of the catalogue and of the adapter
//! it was made under. Whizbang writes it, in one form, and reads back no other.

use serde_json::{Value, json};
use whizbang_backends::models::{
    FILE_MAX_BYTES, FILE_NAME_LIMIT, FILES_LIMIT, INSTALLATION_MAX_BYTES, InstallationRecord,
    InstalledFile, MODEL_NAME_LIMIT, REVISION_LIMIT, Refused, Rule, What,
};
use whizbang_core::contract::Secrets;

use super::{MARKER, entry, file_name, hash, model, none, revision, secrets, shows_nothing_of};

fn installed(name: &str, size: u64, of: &[u8]) -> InstalledFile {
    InstalledFile::new(file_name(name), size, hash(of)).unwrap()
}

/// The record of the entry `tiny` installed under the adapter's revision 7.
fn record() -> InstallationRecord {
    InstallationRecord::of(&entry("tiny"), revision("7"), &none()).unwrap()
}

/// The one form of that record, written here by hand.
fn written() -> String {
    format!(
        concat!(
            r#"{{"format":"installation/1","model":"tiny","catalogue_revision":"2026-11.1","#,
            r#""adapter":"owlv2-sam2","adapter_revision":"7","files":["#,
            r#"{{"name":"LICENSE.txt","size":5,"sha256":"{}"}},"#,
            r#"{{"name":"graph.onnx","size":3,"sha256":"{}"}}]}}"#
        ),
        hash(b"terms"),
        hash(b"abc")
    )
}

fn read(text: &str) -> Result<InstallationRecord, Refused> {
    InstallationRecord::parse(text.as_bytes(), &none())
}

/// The record above as a value, changed, and written compactly in the order of its
/// fields: the one form but for the change.
fn rewritten(change: impl FnOnce(&mut Value)) -> String {
    let mut value: Value = serde_json::from_str(&written()).unwrap();
    change(&mut value);
    let file = |file: &Value| {
        format!(
            r#"{{"name":{},"size":{},"sha256":{}}}"#,
            file["name"], file["size"], file["sha256"]
        )
    };
    let files: Vec<String> = value["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(file)
        .collect();
    format!(
        r#"{{"format":{},"model":{},"catalogue_revision":{},"adapter":{},"adapter_revision":{},"files":[{}]}}"#,
        value["format"],
        value["model"],
        value["catalogue_revision"],
        value["adapter"],
        value["adapter_revision"],
        files.join(",")
    )
}

fn refusal_after(change: impl FnOnce(&mut Value)) -> Refused {
    let refused = read(&rewritten(change)).unwrap_err();
    shows_nothing_of(&refused, &[]);
    refused
}

fn value_refused(at: &str, rule: Rule) -> Refused {
    Refused::Value {
        at: at.to_owned(),
        rule,
    }
}

// --- what a record holds ---------------------------------------------------------------------

#[test]
fn an_installation_record_holds_the_model_its_files_and_the_two_revisions_it_was_made_under() {
    let record = record();
    assert_eq!(record.model().as_str(), "tiny");
    assert_eq!(record.catalogue_revision().as_str(), "2026-11.1");
    assert_eq!(record.adapter().as_str(), "owlv2-sam2");
    assert_eq!(record.adapter_revision().as_str(), "7");
    // The files of the entry, each with its size and its hash, in the order of their
    // names; and nothing of where they came from.
    assert_eq!(
        record.files(),
        [
            installed("LICENSE.txt", 5, b"terms"),
            installed("graph.onnx", 3, b"abc")
        ]
    );
    assert_eq!(record.files()[1].name().as_str(), "graph.onnx");
    assert_eq!(record.files()[1].size(), 3);
    assert_eq!(record.files()[1].sha256(), hash(b"abc"));
    let bytes = record.to_bytes();
    assert_eq!(String::from_utf8(bytes).unwrap(), written());
    assert!(!written().contains("weights.example.org"));
    // Made from its parts, in whatever order its files are given, it is the same record.
    let by_parts = InstallationRecord::new(
        model("tiny"),
        revision("2026-11.1"),
        model("owlv2-sam2"),
        revision("7"),
        vec![
            installed("graph.onnx", 3, b"abc"),
            installed("LICENSE.txt", 5, b"terms"),
        ],
        &none(),
    )
    .unwrap();
    assert_eq!(by_parts, record);
}

#[test]
fn an_installation_record_round_trips() {
    let record = record();
    let bytes = record.to_bytes();
    let again = InstallationRecord::parse(&bytes, &none()).unwrap();
    assert_eq!(again, record);
    assert_eq!(again.to_bytes(), bytes);
    assert_eq!(read(&written()).unwrap(), record);
    assert_eq!(
        InstallationRecord::read_from(written().as_bytes(), &none()).unwrap(),
        record
    );
}

// --- its bound -------------------------------------------------------------------------------

/// The longest record there can be: every name and revision at its limit, the most
/// files, each of the largest size.
fn longest() -> InstallationRecord {
    let files = (0..FILES_LIMIT)
        .map(|at| {
            let name = format!("{at:02}{}", "f".repeat(FILE_NAME_LIMIT - 2));
            installed(&name, FILE_MAX_BYTES, b"x")
        })
        .collect();
    InstallationRecord::new(
        model(&"m".repeat(MODEL_NAME_LIMIT)),
        revision(&"1".repeat(REVISION_LIMIT)),
        model(&"a".repeat(MODEL_NAME_LIMIT)),
        revision(&"2".repeat(REVISION_LIMIT)),
        files,
        &none(),
    )
    .unwrap()
}

#[test]
fn the_longest_record_whizbang_writes_is_under_the_bound_it_reads_back() {
    assert_eq!(INSTALLATION_MAX_BYTES, 32 << 10);
    let bytes = longest().to_bytes();
    // 15,979 bytes: the bound is the next power of two over twice that.
    assert_eq!(bytes.len(), 15_979);
    assert!((bytes.len() as u64) < INSTALLATION_MAX_BYTES);
    assert_eq!(
        InstallationRecord::parse(&bytes, &none()).unwrap(),
        longest()
    );
}

#[test]
fn an_installation_record_over_its_bound_is_refused_before_it_is_read() {
    let too_large = Refused::TooLarge {
        what: What::Installation,
        limit: INSTALLATION_MAX_BYTES,
    };
    // Not a record, and holding a secret the reader was told of: refused for its
    // length, so nothing of it was looked at.
    let mut over = vec![b'x'; INSTALLATION_MAX_BYTES as usize + 1];
    over[..MARKER.len()].copy_from_slice(MARKER.as_bytes());
    assert_eq!(
        InstallationRecord::parse(&over, &secrets()).unwrap_err(),
        too_large
    );
    // At the bound exactly the bytes are looked at, and are refused for what they are.
    assert_eq!(
        InstallationRecord::parse(&over[..INSTALLATION_MAX_BYTES as usize], &secrets())
            .unwrap_err(),
        Refused::HoldsSecret
    );
    assert_eq!(
        InstallationRecord::parse(&over[MARKER.len()..], &secrets()).unwrap_err(),
        Refused::NotJson {
            what: What::Installation,
            line: 1
        }
    );
}

struct Endless {
    given: u64,
}

impl std::io::Read for Endless {
    fn read(&mut self, into: &mut [u8]) -> std::io::Result<usize> {
        into.fill(b' ');
        self.given += into.len() as u64;
        Ok(into.len())
    }
}

struct Failing;

impl std::io::Read for Failing {
    fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "/srv/weights/tiny/RECORD: not there",
        ))
    }
}

#[test]
fn reading_a_record_asks_its_source_for_no_more_than_the_bound_and_one_byte_and_keeps_no_path() {
    let mut endless = Endless { given: 0 };
    assert_eq!(
        InstallationRecord::read_from(&mut endless, &none()).unwrap_err(),
        Refused::TooLarge {
            what: What::Installation,
            limit: INSTALLATION_MAX_BYTES
        }
    );
    assert_eq!(endless.given, INSTALLATION_MAX_BYTES + 1);
    let refused = InstallationRecord::read_from(Failing, &none()).unwrap_err();
    assert_eq!(
        refused,
        Refused::Unreadable {
            what: What::Installation,
            kind: std::io::ErrorKind::NotFound
        }
    );
    shows_nothing_of(&refused, &["weights", "RECORD", "/srv"]);
}

// --- read strictly ---------------------------------------------------------------------------

#[test]
fn a_record_is_read_back_only_in_the_one_form_whizbang_writes() {
    let one_form = written();
    assert!(read(&one_form).is_ok());
    let other = Refused::OtherForm {
        what: What::Installation,
    };
    let value: Value = serde_json::from_str(&one_form).unwrap();
    let files_swapped = rewritten(|v| v["files"].as_array_mut().unwrap().reverse());
    let cases = [
        // White space JSON allows, anywhere.
        format!("{one_form}\n"),
        format!(" {one_form}"),
        one_form.replace("\"model\":", "\"model\": "),
        serde_json::to_string_pretty(&value).unwrap(),
        // The fields in another order (a JSON value's own order is by key).
        value.to_string(),
        // A character spelled another way.
        one_form.replace("\"tiny\"", "\"t\\u0069ny\""),
        one_form.replace("graph.onnx", "graph\\u002eonnx"),
        // A number spelled another way that reads as the same number.
        one_form.replace("\"size\":5", "\"size\":5 "),
        // The files in another order than that of their names.
        files_swapped,
    ];
    for text in &cases {
        assert_ne!(text, &one_form);
        let refused = read(text).unwrap_err();
        assert_eq!(refused, other, "{text}");
        shows_nothing_of(&refused, &["tiny"]);
    }
}

#[test]
fn a_record_that_is_not_json_is_refused() {
    let whole = written();
    for text in [
        "",
        "oops",
        &whole[..whole.len() - 1],
        &format!("{whole}{whole}"),
        &format!("{whole}]"),
    ] {
        assert_eq!(
            read(text).unwrap_err(),
            Refused::NotJson {
                what: What::Installation,
                line: 1
            },
            "{text:?}"
        );
    }
}

#[test]
fn a_record_with_an_unknown_field_a_missing_one_or_one_given_twice_is_refused() {
    let whole = written();
    let hash_text = format!(",\"sha256\":\"{}\"", hash(b"abc"));
    let twice = Refused::Twice {
        what: What::Installation,
        line: 1,
    };
    let cases = [
        // An unknown field, in the record and in a file.
        (
            whole.replacen("\"files\":", "\"licence\":\"MIT\",\"files\":", 1),
            value_refused("", Rule::Unknown),
        ),
        (
            whole.replacen("\"size\":5,", "\"size\":5,\"from\":\"x\",", 1),
            value_refused("files[0]", Rule::Unknown),
        ),
        // A missing field, each of them.
        (
            whole.replacen("\"format\":\"installation/1\",", "", 1),
            value_refused("format", Rule::Missing),
        ),
        (
            whole.replacen("\"model\":\"tiny\",", "", 1),
            value_refused("model", Rule::Missing),
        ),
        (
            whole.replacen("\"catalogue_revision\":\"2026-11.1\",", "", 1),
            value_refused("catalogue_revision", Rule::Missing),
        ),
        (
            whole.replacen("\"adapter\":\"owlv2-sam2\",", "", 1),
            value_refused("adapter", Rule::Missing),
        ),
        (
            whole.replacen("\"adapter_revision\":\"7\",", "", 1),
            value_refused("adapter_revision", Rule::Missing),
        ),
        (
            whole[..whole.find(",\"files\"").unwrap()].to_owned() + "}",
            value_refused("files", Rule::Missing),
        ),
        (
            whole.replacen("\"name\":\"LICENSE.txt\",", "", 1),
            value_refused("files[0].name", Rule::Missing),
        ),
        (
            whole.replacen("\"size\":5,", "", 1),
            value_refused("files[0].size", Rule::Missing),
        ),
        (
            whole.replacen(&hash_text, "", 1),
            value_refused("files[1].sha256", Rule::Missing),
        ),
        // A field given twice, in the record and in a file.
        (
            whole.replacen(
                "\"model\":\"tiny\",",
                "\"model\":\"tiny\",\"model\":\"tiny\",",
                1,
            ),
            twice.clone(),
        ),
        (
            whole.replacen("\"size\":5,", "\"size\":5,\"size\":5,", 1),
            twice,
        ),
        // A value of another kind.
        (
            whole.replacen("\"size\":5", "\"size\":\"5\"", 1),
            value_refused("files[0].size", Rule::Kind),
        ),
        (
            whole.replacen("\"size\":5", "\"size\":5.5", 1),
            value_refused("files[0].size", Rule::Kind),
        ),
        (
            whole.replacen("\"size\":5", "\"size\":-5", 1),
            value_refused("files[0].size", Rule::Kind),
        ),
        (
            whole.replacen("\"model\":\"tiny\"", "\"model\":7", 1),
            value_refused("model", Rule::Kind),
        ),
        (
            whole.replacen("\"model\":\"tiny\"", "\"model\":null", 1),
            value_refused("model", Rule::Kind),
        ),
        (
            whole.replacen("\"files\":[", "\"files\":[7,", 1),
            value_refused("files[0]", Rule::Kind),
        ),
        ("[]".to_owned(), value_refused("", Rule::Kind)),
        ("null".to_owned(), value_refused("", Rule::Kind)),
    ];
    for (text, expected) in &cases {
        assert_ne!(text, &whole);
        let refused = read(text).unwrap_err();
        assert_eq!(&refused, expected, "{text}");
        shows_nothing_of(&refused, &["tiny", "LICENSE"]);
    }
}

#[test]
fn a_record_of_another_format_is_refused() {
    for format in ["installation/2", "catalogue/1", ""] {
        assert_eq!(
            refusal_after(|v| v["format"] = json!(format)),
            value_refused("format", Rule::Format)
        );
    }
}

#[test]
fn a_record_with_a_hash_of_the_wrong_length_is_refused() {
    let good = hash(b"abc").to_string();
    for sha256 in [
        good[..good.len() - 1].to_owned(),
        format!("{good}0"),
        good.trim_start_matches("sha256:").to_owned(),
        good.to_uppercase().replace("SHA256:", "sha256:"),
        String::new(),
    ] {
        let refused = refusal_after(|v| v["files"][1]["sha256"] = json!(sha256));
        assert_eq!(
            refused,
            value_refused("files[1].sha256", Rule::Hash),
            "{sha256:?}"
        );
        if sha256.len() > 8 {
            shows_nothing_of(&refused, &[&sha256]);
        }
    }
}

#[test]
fn a_record_with_a_file_name_that_is_not_one_plain_name_is_refused() {
    for name in [
        "",
        ".",
        "..",
        "../graph.onnx",
        "sub/graph.onnx",
        "/graph.onnx",
        ".hidden",
        "a b",
        "a\\b",
        "a\u{0}b",
    ] {
        let refused = refusal_after(|v| v["files"][0]["name"] = json!(name));
        assert_eq!(
            refused,
            value_refused("files[0].name", Rule::FileName),
            "{name:?}"
        );
        if name.len() > 3 {
            shows_nothing_of(&refused, &[name]);
        }
    }
    for name in ["", "..", "Tiny", "models/tiny"] {
        assert_eq!(
            refusal_after(|v| v["model"] = json!(name)),
            value_refused("model", Rule::ModelName)
        );
        assert_eq!(
            refusal_after(|v| v["adapter"] = json!(name)),
            value_refused("adapter", Rule::ModelName)
        );
    }
    for text in ["", "1 2", "../7"] {
        assert_eq!(
            refusal_after(|v| v["catalogue_revision"] = json!(text)),
            value_refused("catalogue_revision", Rule::Revision)
        );
        assert_eq!(
            refusal_after(|v| v["adapter_revision"] = json!(text)),
            value_refused("adapter_revision", Rule::Revision)
        );
    }
}

#[test]
fn a_record_with_a_size_of_zero_is_refused_and_so_is_one_past_the_most_a_file_may_be() {
    for size in [0, FILE_MAX_BYTES + 1] {
        assert_eq!(
            refusal_after(|v| v["files"][0]["size"] = json!(size)),
            value_refused("files[0].size", Rule::Size)
        );
        assert_eq!(
            InstalledFile::new(file_name("f"), size, hash(b"f")).unwrap_err(),
            value_refused("", Rule::Size)
        );
    }
    for size in [1, FILE_MAX_BYTES] {
        let text = rewritten(|v| v["files"][0]["size"] = json!(size));
        assert_eq!(read(&text).unwrap().files()[0].size(), size);
    }
}

#[test]
fn a_record_holds_one_to_a_bounded_count_of_files_each_named_once_in_either_case() {
    let with_files = |files: Vec<InstalledFile>| {
        InstallationRecord::new(
            model("tiny"),
            revision("1"),
            model("adapter"),
            revision("1"),
            files,
            &none(),
        )
    };
    let many = |count: usize| -> Vec<InstalledFile> {
        (0..count)
            .map(|at| installed(&format!("part-{at:02}"), 1, b"x"))
            .collect()
    };
    assert!(with_files(many(1)).is_ok());
    assert!(with_files(many(FILES_LIMIT)).is_ok());
    assert_eq!(
        with_files(many(0)).unwrap_err(),
        value_refused("files", Rule::NoFiles)
    );
    assert_eq!(
        with_files(many(FILES_LIMIT + 1)).unwrap_err(),
        value_refused("files", Rule::Count { most: FILES_LIMIT })
    );
    for (first, second) in [("graph.onnx", "graph.onnx"), ("graph.onnx", "GRAPH.onnx")] {
        assert_eq!(
            with_files(vec![installed(first, 1, b"x"), installed(second, 2, b"y")]).unwrap_err(),
            value_refused("files", Rule::NamedTwice)
        );
    }
    // Read: the count is looked at before any file is, and a name twice is refused
    // though the bytes are otherwise in the one form.
    assert_eq!(
        refusal_after(|v| v["files"] = json!([])),
        value_refused("files", Rule::NoFiles)
    );
    assert_eq!(
        refusal_after(|v| {
            let one = v["files"][0].clone();
            v["files"] = Value::Array(vec![one; FILES_LIMIT + 1]);
        }),
        value_refused("files", Rule::Count { most: FILES_LIMIT })
    );
    assert_eq!(
        refusal_after(|v| v["files"][1]["name"] = json!("LICENSE.txt")),
        value_refused("files", Rule::NamedTwice)
    );
    assert_eq!(
        refusal_after(|v| v["files"][1]["name"] = json!("license.TXT")),
        value_refused("files", Rule::NamedTwice)
    );
}

/// One change to a record as a value.
type Change = Box<dyn Fn(&mut Value)>;

#[test]
fn a_list_of_files_over_its_bound_is_refused_for_its_count_before_any_file_is_looked_at() {
    let whole = written();
    let start = whole.find("\"files\":[").unwrap() + "\"files\":[".len();
    let text = format!(
        "{}{}{}",
        &whole[..start],
        "7,".repeat(FILES_LIMIT),
        &whole[start..]
    );
    assert_eq!(
        read(&text).unwrap_err(),
        value_refused("files", Rule::Count { most: FILES_LIMIT })
    );
}

// --- a secret is never read into a name, and never written -----------------------------------

#[test]
fn a_record_that_holds_a_secret_anywhere_is_refused_whole_and_not_shown() {
    let changes: Vec<Change> = vec![
        Box::new(|v| v["model"] = json!(format!("tiny-{MARKER}"))),
        Box::new(|v| v["files"][0]["name"] = json!(format!("A{MARKER}.onnx"))),
        Box::new(|v| v["catalogue_revision"] = json!(MARKER)),
        Box::new(|v| v["adapter"] = json!(MARKER)),
        Box::new(|v| v["adapter_revision"] = json!(MARKER)),
        Box::new(|v| v["format"] = json!(MARKER)),
        Box::new(|v| v["files"][0]["sha256"] = json!(MARKER)),
    ];
    for (at, change) in changes.iter().enumerate() {
        let text = rewritten(change);
        assert!(text.contains(MARKER), "case {at}");
        let spelled = text.replace(MARKER, &format!("\\u0073{}", &MARKER[1..]));
        for text in [&text, &spelled] {
            let refused = InstallationRecord::parse(text.as_bytes(), &secrets()).unwrap_err();
            assert_eq!(refused, Refused::HoldsSecret, "case {at}: {text}");
            shows_nothing_of(&refused, &[MARKER, &MARKER[..4]]);
        }
    }
    // The first two are records when the reader is told of no secret.
    assert!(read(&rewritten(&changes[0])).is_ok());
    assert!(read(&rewritten(&changes[1])).is_ok());
}

#[test]
fn a_record_that_would_spell_a_secret_is_not_made() {
    // Where a record is made, not read: its parts were each made by a constructor
    // that was told of no secret, or of another.
    let told = secrets();
    let named = InstallationRecord::new(
        model(&format!("tiny-{MARKER}")),
        revision("1"),
        model("adapter"),
        revision("1"),
        vec![installed("f", 1, b"x")],
        &told,
    );
    assert_eq!(named.unwrap_err(), Refused::HoldsSecret);
    let file = InstallationRecord::new(
        model("tiny"),
        revision("1"),
        model("adapter"),
        revision("1"),
        vec![installed(&format!("{MARKER}.bin"), 1, b"x")],
        &told,
    );
    assert_eq!(file.unwrap_err(), Refused::HoldsSecret);
    assert_eq!(
        InstallationRecord::of(&entry("tiny"), revision(MARKER), &told).unwrap_err(),
        Refused::HoldsSecret
    );
    // A secret that no part holds and the written record spells, across two fields.
    let across = Secrets::new(["tiny\",\"catalogue_revision\"".to_owned()]).unwrap();
    assert_eq!(
        InstallationRecord::of(&entry("tiny"), revision("7"), &across).unwrap_err(),
        Refused::HoldsSecret
    );
    assert!(InstallationRecord::of(&entry("tiny"), revision("7"), &told).is_ok());
}
