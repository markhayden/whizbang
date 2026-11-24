//! The catalogue (ADR 0017, "The catalogue"): the models that can be installed, each
//! with its files, their sizes and hashes, and the origins they may be downloaded from.
//! It is a file a person writes, read strictly.

use serde_json::{Value, json};
use whizbang_backends::models::{
    CATALOGUE_MAX_BYTES, Carried, Catalogue, CatalogueFile, Entry, FEATURES_LIMIT, FILE_MAX_BYTES,
    FILES_LIMIT, InstallationRecord, MODELS_LIMIT, ORIGINS_LIMIT, PLATFORMS_LIMIT, Platform,
    Refused, Rule, TITLE_LIMIT, Title, What,
};
use whizbang_core::egress::{Address, NotAllowed};

use super::{MARKER, entry, file, file_name, hash, none, origin, parts, secrets, shows_nothing_of};

/// A catalogue of one model with two files, as a person would write it.
fn written() -> Value {
    json!({
        "format": "catalogue/2",
        "models": [{
            "name": "tiny",
            "title": "Tiny, for tests",
            "revision": "2026-11.1",
            "adapter": "owlv2-sam2",
            "licence": "Apache-2.0",
            "platforms": ["macos-arm64", "linux-x86_64"],
            "features": ["phrase", "box"],
            "origins": ["https://weights.example.org"],
            "files": [
                {
                    "name": "graph.onnx",
                    "size": 3,
                    "sha256": hash(b"abc").to_string(),
                    "from": "https://weights.example.org/files/graph.onnx"
                },
                {
                    "name": "LICENSE.txt",
                    "size": 5,
                    "sha256": hash(b"terms").to_string(),
                    "from": "https://weights.example.org/files/LICENSE.txt"
                }
            ]
        }]
    })
}

fn read(value: &Value) -> Result<Catalogue, Refused> {
    Catalogue::parse(value.to_string().as_bytes(), &none())
}

/// The refusal of the catalogue above after one change to it.
fn refusal_after(change: impl FnOnce(&mut Value)) -> Refused {
    let mut value = written();
    change(&mut value);
    let refused = read(&value).unwrap_err();
    shows_nothing_of(&refused, &[]);
    refused
}

fn value_refused(at: &str, rule: Rule) -> Refused {
    Refused::Value {
        at: at.to_owned(),
        rule,
    }
}

// --- what a catalogue holds ------------------------------------------------------------------

#[test]
fn a_catalogue_is_read_as_its_models_and_their_files() {
    let catalogue = read(&written()).unwrap();
    assert_eq!(catalogue.entries().len(), 1);
    let tiny = catalogue.entry("tiny").unwrap();
    assert_eq!(tiny, &entry("tiny"));
    assert_eq!(tiny.name().as_str(), "tiny");
    assert_eq!(tiny.revision().as_str(), "2026-11.1");
    assert_eq!(tiny.adapter().as_str(), "owlv2-sam2");
    assert_eq!(tiny.licence().as_str(), "Apache-2.0");
    // Features and files are kept in the order of their names, whatever order they were
    // written in: upper case comes first, as bytes are ordered.
    assert_eq!(tiny.features().collect::<Vec<_>>(), ["box", "phrase"]);
    assert!(tiny.has("phrase") && !tiny.has("point"));
    let names: Vec<&str> = tiny
        .files()
        .iter()
        .map(|file| file.name().as_str())
        .collect();
    assert_eq!(names, ["LICENSE.txt", "graph.onnx"]);
    let graph = &tiny.files()[1];
    assert_eq!(graph.size(), 3);
    assert_eq!(graph.sha256(), hash(b"abc"));
    assert_eq!(
        graph.from().unwrap().origin(),
        &origin("https://weights.example.org")
    );
    assert_eq!(graph.from().unwrap().rest(), "/files/graph.onnx");
    assert_eq!(tiny.origins(), [origin("https://weights.example.org")]);
    assert_eq!(tiny.bytes(), 8);
    assert!(catalogue.entry("sam3").is_none());
}

#[test]
fn an_entry_of_the_second_version_holds_a_title_and_its_platforms_and_the_first_version_is_refused()
{
    let catalogue = read(&written()).unwrap();
    let tiny = catalogue.entry("tiny").unwrap();
    assert_eq!(tiny.title().as_str(), "Tiny, for tests");
    // The platforms are kept in the order of their names, whatever order they were
    // written in, and each is one this reader knows.
    assert_eq!(
        tiny.platforms(),
        [Platform::LinuxX86_64, Platform::MacosArm64]
    );
    let names: Vec<&str> = tiny.platforms().iter().map(|p| p.name()).collect();
    assert_eq!(names, ["linux-x86_64", "macos-arm64"]);
    assert!(tiny.runs_on(Platform::MacosArm64) && tiny.runs_on(Platform::LinuxX86_64));
    let mut one = written();
    one["models"][0]["platforms"] = json!(["macos-arm64"]);
    let mac_only = read(&one).unwrap();
    let mac_only = mac_only.entry("tiny").unwrap();
    assert!(mac_only.runs_on(Platform::MacosArm64) && !mac_only.runs_on(Platform::LinuxX86_64));
    // Both are written again, in the format's order, and read back.
    let bytes = String::from_utf8(catalogue.to_bytes()).unwrap();
    assert!(
        bytes.starts_with(
            "{\"format\":\"catalogue/2\",\"models\":[{\"name\":\"tiny\",\
             \"title\":\"Tiny, for tests\",\"revision\":"
        ),
        "{bytes}"
    );
    assert!(
        bytes.contains(
            "\"licence\":\"Apache-2.0\",\"platforms\":[\"linux-x86_64\",\"macos-arm64\"],\
             \"features\":"
        ),
        "{bytes}"
    );
    assert_eq!(
        Catalogue::parse(bytes.as_bytes(), &none()).unwrap(),
        catalogue
    );
    // Each of the two platforms has its name, and no other name is a platform; this
    // build's is one of them, on the two the fast tier runs on.
    for platform in Platform::ALL {
        assert_eq!(Platform::named(platform.name()), Some(platform));
    }
    for name in [
        "",
        "macos",
        "Macos-arm64",
        "macos-arm64 ",
        "linux-arm64",
        "windows-x86_64",
    ] {
        assert_eq!(Platform::named(name), None, "{name}");
    }
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        assert_eq!(Platform::this(), Some(Platform::MacosArm64));
    }
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        assert_eq!(Platform::this(), Some(Platform::LinuxX86_64));
    }

    // The first version is no longer read: no catalogue of it was shipped. It is
    // refused for its format, with or without the two new fields.
    let mut first = written();
    first["format"] = json!("catalogue/1");
    assert_eq!(
        read(&first).unwrap_err(),
        value_refused("format", Rule::Format)
    );
    let fields = first["models"][0].as_object_mut().unwrap();
    fields.remove("title");
    fields.remove("platforms");
    assert_eq!(
        read(&first).unwrap_err(),
        value_refused("format", Rule::Format)
    );
    // And an entry of the second version has both.
    for field in ["title", "platforms"] {
        let refused = refusal_after(|v| {
            v["models"][0].as_object_mut().unwrap().remove(field);
        });
        assert_eq!(
            refused,
            value_refused(&format!("models[0].{field}"), Rule::Missing)
        );
    }

    // A title is 1 to 100 printable ASCII characters and spaces, with no space at
    // either end; one that holds a secret is refused as a secret.
    let longest = "t".repeat(TITLE_LIMIT);
    let mut long = written();
    long["models"][0]["title"] = json!(longest);
    assert_eq!(
        read(&long).unwrap().entry("tiny").unwrap().title().as_str(),
        longest
    );
    for title in [
        json!(""),
        json!(" Tiny"),
        json!("Tiny "),
        json!("Tiny\nfor tests"),
        json!("Tiny \u{e9}"),
        json!("t".repeat(TITLE_LIMIT + 1)),
    ] {
        assert_eq!(
            refusal_after(|v| v["models"][0]["title"] = title.clone()),
            value_refused("models[0].title", Rule::Title),
            "{title}"
        );
    }
    assert_eq!(
        refusal_after(|v| v["models"][0]["title"] = json!(7)),
        value_refused("models[0].title", Rule::Kind)
    );
    let mut secret = written();
    secret["models"][0]["title"] = json!(format!("Tiny {MARKER}"));
    assert_eq!(
        Catalogue::parse(secret.to_string().as_bytes(), &secrets()).unwrap_err(),
        Refused::HoldsSecret
    );
    assert_eq!(
        Title::new(&format!("Tiny {MARKER}"), &secrets()),
        Err(Refused::HoldsSecret)
    );

    // The platforms: one at least, each a platform's name, each named once, and at
    // most as many as the bound, counted before any is looked at.
    assert_eq!(
        refusal_after(|v| v["models"][0]["platforms"] = json!([])),
        value_refused("models[0].platforms", Rule::NoPlatforms)
    );
    assert_eq!(
        refusal_after(|v| v["models"][0]["platforms"] = json!(["macos-arm64", "linux-arm64"])),
        value_refused("models[0].platforms[1]", Rule::Platform)
    );
    assert_eq!(
        refusal_after(|v| v["models"][0]["platforms"] = json!(["macos-arm64", 7])),
        value_refused("models[0].platforms[1]", Rule::Kind)
    );
    assert_eq!(
        refusal_after(|v| v["models"][0]["platforms"] = json!(["macos-arm64", "macos-arm64"])),
        value_refused("models[0].platforms", Rule::NamedTwice)
    );
    assert_eq!(
        refusal_after(|v| v["models"][0]["platforms"] = json!(vec!["nowhere"; PLATFORMS_LIMIT + 1])),
        value_refused(
            "models[0].platforms",
            Rule::Count {
                most: PLATFORMS_LIMIT
            }
        )
    );
    // The same rules hold an entry that is made and not read.
    let mut made = parts("tiny");
    made.platforms = Vec::new();
    assert_eq!(
        Entry::new(made, &none()).unwrap_err(),
        value_refused("platforms", Rule::NoPlatforms)
    );
    let mut made = parts("tiny");
    made.platforms = vec![Platform::MacosArm64, Platform::MacosArm64];
    assert_eq!(
        Entry::new(made, &none()).unwrap_err(),
        value_refused("platforms", Rule::NamedTwice)
    );
    // An installation is of the files and the two revisions: a title or a platform
    // changed alone, against the rule that whoever changes an entry changes its
    // revision, is the same installation.
    let mut retitled = parts("tiny");
    retitled.title = Title::new("Another title", &none()).unwrap();
    retitled.platforms = vec![Platform::MacosArm64];
    let retitled = Entry::new(retitled, &none()).unwrap();
    assert_ne!(retitled, entry("tiny"));
    let record =
        |entry: &Entry| InstallationRecord::of(entry, super::revision("7"), &none()).unwrap();
    assert_eq!(record(&retitled), record(&entry("tiny")));
}

#[test]
fn an_entry_may_say_that_a_download_from_one_of_its_origins_needs_a_token() {
    // Plan 1C, task N4, D12: the field may be left out, and then no download of the
    // model needs a token; given, it is one of the entry's origins, written as an
    // origin. Nothing of the token is in a catalogue: only that one is needed.
    let plain = read(&written()).unwrap();
    assert_eq!(plain.entry("tiny").unwrap().needs_token(), None);
    let mut value = written();
    value["models"][0]["needs_token"] = json!("https://weights.example.org");
    let catalogue = read(&value).unwrap();
    let tiny = catalogue.entry("tiny").unwrap();
    assert_eq!(
        tiny.needs_token(),
        Some(&origin("https://weights.example.org"))
    );
    // Written again as it was read, and read back the same; one with none written
    // without the field.
    assert_eq!(
        Catalogue::parse(&catalogue.to_bytes(), &none()).unwrap(),
        catalogue
    );
    let bytes = String::from_utf8(catalogue.to_bytes()).unwrap();
    assert!(
        bytes.contains("\"needs_token\":\"https://weights.example.org\""),
        "{bytes}"
    );
    assert!(
        !String::from_utf8(plain.to_bytes())
            .unwrap()
            .contains("needs_token")
    );
    assert_eq!(
        entry("tiny")
            .needing_a_token_at(origin("https://weights.example.org"))
            .unwrap(),
        *tiny
    );
    // An origin the entry does not name, and what is not an origin, are refused.
    let refused = refusal_after(|value| {
        value["models"][0]["needs_token"] = json!("https://elsewhere.example.org");
    });
    assert_eq!(
        refused,
        value_refused("models[0].needs_token", Rule::Elsewhere)
    );
    let refused = refusal_after(|value| {
        value["models"][0]["needs_token"] = json!("not an origin");
    });
    assert!(matches!(refused, Refused::Value { ref at, .. } if at == "models[0].needs_token"));
    let refused = refusal_after(|value| value["models"][0]["needs_token"] = json!(7));
    assert_eq!(refused, value_refused("models[0].needs_token", Rule::Kind));
    assert!(
        entry("tiny")
            .needing_a_token_at(origin("https://elsewhere.example.org"))
            .is_err()
    );
}

#[test]
fn a_file_may_name_a_text_the_binary_carries_in_place_of_an_address() {
    // Plan m6, task W1: `carried` names a text the binary carries, and the file then has
    // no address and needs no origin. Its size and hash are pinned as any file's are.
    let terms = Carried::Apache2.bytes();
    let mut value = written();
    value["models"][0]["files"][1] = json!({
        "name": "LICENSE",
        "size": terms.len(),
        "sha256": hash(terms).to_string(),
        "carried": "apache-2.0"
    });
    let catalogue = read(&value).unwrap();
    let tiny = catalogue.entry("tiny").unwrap();
    let licence = tiny
        .files()
        .iter()
        .find(|file| file.name().as_str() == "LICENSE")
        .unwrap();
    assert_eq!(licence.carried(), Some(Carried::Apache2));
    assert_eq!(licence.from(), None);
    assert_eq!(licence.size(), terms.len() as u64);
    assert_eq!(licence.sha256(), hash(terms));
    let graph = tiny
        .files()
        .iter()
        .find(|file| file.name().as_str() == "graph.onnx")
        .unwrap();
    assert_eq!(graph.carried(), None);
    assert!(graph.from().is_some());
    // Written with `carried` in the place of `from`, and read back the same.
    let bytes = String::from_utf8(catalogue.to_bytes()).unwrap();
    assert!(
        bytes.contains(&format!(
            "{{\"name\":\"LICENSE\",\"size\":{},\"sha256\":\"{}\",\"carried\":\"apache-2.0\"}}",
            terms.len(),
            hash(terms)
        )),
        "{bytes}"
    );
    assert_eq!(
        Catalogue::parse(&catalogue.to_bytes(), &none()).unwrap(),
        catalogue
    );
    // An entry whose every file is carried names no origin, and is made.
    let mut parts = parts("tiny");
    parts.origins = Vec::new();
    parts.files = vec![
        CatalogueFile::from_binary(
            file_name("LICENSE"),
            terms.len() as u64,
            hash(terms),
            Carried::Apache2,
        )
        .unwrap(),
    ];
    let alone = Entry::new(parts, &none()).unwrap();
    assert_eq!(alone.files()[0].carried(), Some(Carried::Apache2));
    // The one carried text is named `apache-2.0`, and no other name is one.
    assert_eq!(Carried::named("apache-2.0"), Some(Carried::Apache2));
    assert_eq!(Carried::Apache2.name(), "apache-2.0");
    for other in ["Apache-2.0", "apache", "mit", ""] {
        assert_eq!(Carried::named(other), None, "{other}");
    }
    let refused = refusal_after(|value| {
        value["models"][0]["files"][1]["carried"] = json!("mit");
        value["models"][0]["files"][1]
            .as_object_mut()
            .unwrap()
            .remove("from");
    });
    assert_eq!(
        refused,
        value_refused("models[0].files[1].carried", Rule::Carried)
    );
    // Both, or neither: a file comes from one place.
    let refused = refusal_after(|value| {
        value["models"][0]["files"][1]["carried"] = json!("apache-2.0");
    });
    assert_eq!(
        refused,
        value_refused("models[0].files[1]", Rule::TwoSources)
    );
    let refused = refusal_after(|value| {
        value["models"][0]["files"][1]
            .as_object_mut()
            .unwrap()
            .remove("from");
    });
    assert_eq!(
        refused,
        value_refused("models[0].files[1].from", Rule::Missing)
    );
    let refused = refusal_after(|value| {
        value["models"][0]["files"][1]["carried"] = json!(7);
        value["models"][0]["files"][1]
            .as_object_mut()
            .unwrap()
            .remove("from");
    });
    assert_eq!(
        refused,
        value_refused("models[0].files[1].carried", Rule::Kind)
    );
    // A carried file's size is held to its rule as any file's is.
    assert_eq!(
        CatalogueFile::from_binary(file_name("LICENSE"), 0, hash(terms), Carried::Apache2)
            .unwrap_err(),
        Refused::Value {
            at: String::new(),
            rule: Rule::Size
        }
    );
}

#[test]
fn the_apache_text_the_binary_carries_is_the_terms_of_whizbangs_own_licence() {
    // Whizbang's `LICENSE` from its first line to the end of the terms, and not the
    // appendix, which names whizbang's own licensor.
    let licence = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../LICENSE")).unwrap();
    let terms = Carried::Apache2.bytes();
    assert!(licence.starts_with(terms));
    assert!(terms.starts_with(b"Apache License\nVersion 2.0, January 2004\n"));
    assert!(terms.ends_with(b"\nEND OF TERMS AND CONDITIONS\n"));
    let text = String::from_utf8(terms.to_vec()).unwrap();
    assert!(!text.contains("APPENDIX") && !text.contains("Copyright 2"));
    assert_eq!(text.matches("END OF TERMS AND CONDITIONS").count(), 1);
}

#[test]
fn a_catalogue_round_trips() {
    let mut second = parts("sam3");
    second.features = Vec::new();
    second.origins = vec![
        origin("https://weights.example.org"),
        origin("http://mirror.example.net:8080"),
    ];
    second.files.push(
        CatalogueFile::new(
            file_name("vision.onnx.data"),
            FILE_MAX_BYTES,
            hash(b"big"),
            Address::of("http://mirror.example.net:8080/v?sig=abc", &none()).unwrap(),
            &none(),
        )
        .unwrap(),
    );
    let catalogue = Catalogue::new(
        vec![Entry::new(second, &none()).unwrap(), entry("tiny")],
        &none(),
    )
    .unwrap();
    let bytes = catalogue.to_bytes();
    let again = Catalogue::parse(&bytes, &none()).unwrap();
    assert_eq!(again, catalogue);
    // What is written is written one way: the same catalogue gives the same bytes.
    assert_eq!(again.to_bytes(), bytes);
    // Models are kept in the order of their names.
    let names: Vec<&str> = again.entries().iter().map(|e| e.name().as_str()).collect();
    assert_eq!(names, ["sam3", "tiny"]);
    // And what a person wrote reads as the same catalogue as what whizbang writes of it.
    let read_back = read(&written()).unwrap();
    assert_eq!(
        Catalogue::parse(&read_back.to_bytes(), &none()).unwrap(),
        read_back
    );
    // An empty catalogue is one, and round-trips.
    let empty = Catalogue::new(Vec::new(), &none()).unwrap();
    assert_eq!(empty.to_bytes(), br#"{"format":"catalogue/2","models":[]}"#);
    assert_eq!(Catalogue::parse(&empty.to_bytes(), &none()).unwrap(), empty);
}

#[test]
fn a_person_may_write_a_catalogue_in_any_way_json_allows() {
    let compact = written().to_string();
    let pretty = serde_json::to_string_pretty(&written()).unwrap();
    assert_ne!(compact, pretty);
    let escaped = compact.replace("\"tiny\"", "\"t\\u0069ny\"");
    assert_ne!(compact, escaped);
    for text in [&compact, &pretty, &escaped, &format!("  {compact}\n\n")] {
        assert_eq!(
            Catalogue::parse(text.as_bytes(), &none()).unwrap(),
            read(&written()).unwrap()
        );
    }
    // The host of an address in either case, and its port written or not, is one address.
    let mut value = written();
    value["models"][0]["origins"][0] = json!("HTTPS://Weights.Example.org:443");
    value["models"][0]["files"][0]["from"] =
        json!("https://WEIGHTS.example.org:443/files/graph.onnx");
    assert_eq!(read(&value).unwrap(), read(&written()).unwrap());
}

// --- its bound -------------------------------------------------------------------------------

#[test]
fn a_catalogue_over_its_bound_is_refused_before_it_is_read() {
    assert_eq!(CATALOGUE_MAX_BYTES, 1 << 20);
    let too_large = Refused::TooLarge {
        what: What::Catalogue,
        limit: CATALOGUE_MAX_BYTES,
    };
    // Not JSON, and holding a secret the reader was told of: it is refused for its
    // length, so nothing of it was looked at.
    let mut over = vec![b'x'; CATALOGUE_MAX_BYTES as usize + 1];
    over[..MARKER.len()].copy_from_slice(MARKER.as_bytes());
    assert_eq!(Catalogue::parse(&over, &secrets()).unwrap_err(), too_large);
    // A catalogue that would be read, padded past the bound with the white space JSON
    // allows, is refused; padded to the bound exactly it is read.
    let text = written().to_string();
    let padded = |length: usize| format!("{text}{}", " ".repeat(length - text.len()));
    let at = padded(CATALOGUE_MAX_BYTES as usize);
    assert_eq!(
        Catalogue::parse(at.as_bytes(), &none()).unwrap(),
        read(&written()).unwrap()
    );
    let past = padded(CATALOGUE_MAX_BYTES as usize + 1);
    assert_eq!(
        Catalogue::parse(past.as_bytes(), &none()).unwrap_err(),
        too_large
    );
}

/// A source that never ends, and counts what it was asked for.
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

#[test]
fn reading_a_catalogue_asks_its_source_for_no_more_than_the_bound_and_one_byte() {
    let mut endless = Endless { given: 0 };
    assert_eq!(
        Catalogue::read_from(&mut endless, &none()).unwrap_err(),
        Refused::TooLarge {
            what: What::Catalogue,
            limit: CATALOGUE_MAX_BYTES
        }
    );
    assert_eq!(endless.given, CATALOGUE_MAX_BYTES + 1);
    // What can be read whole is read as `parse` reads it.
    let text = written().to_string();
    assert_eq!(
        Catalogue::read_from(text.as_bytes(), &none()).unwrap(),
        read(&written()).unwrap()
    );
}

/// A source that fails, with a path in its error.
struct Failing;

impl std::io::Read for Failing {
    fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "/srv/weights/catalogue.json: denied",
        ))
    }
}

#[test]
fn a_catalogue_that_cannot_be_read_is_refused_by_the_kind_of_failure_and_no_path() {
    let refused = Catalogue::read_from(Failing, &none()).unwrap_err();
    assert_eq!(
        refused,
        Refused::Unreadable {
            what: What::Catalogue,
            kind: std::io::ErrorKind::PermissionDenied
        }
    );
    shows_nothing_of(&refused, &["weights", "catalogue.json", "/srv"]);
}

// --- read strictly ---------------------------------------------------------------------------

#[test]
fn a_catalogue_that_is_not_json_is_refused_by_its_line() {
    let not_json = |text: &str, line: usize| {
        let refused = Catalogue::parse(text.as_bytes(), &none()).unwrap_err();
        assert_eq!(
            refused,
            Refused::NotJson {
                what: What::Catalogue,
                line
            },
            "{text:?}"
        );
        shows_nothing_of(&refused, &["tiny", "oops"]);
    };
    not_json("", 1);
    not_json("oops", 1);
    not_json("{\n\"format\": \"catalogue/2\",\n\"models\": [oops]}", 3);
    let whole = written().to_string();
    not_json(&whole[..whole.len() - 1], 1);
    not_json(&format!("{whole}{whole}"), 1);
    not_json(&format!("{whole} tail"), 1);
    not_json(&format!("\u{feff}{whole}"), 1);
    // Bytes that are not UTF-8, inside a text.
    let mut bytes = whole.clone().into_bytes();
    let at = whole.find("tiny").unwrap();
    bytes[at] = 0xff;
    assert_eq!(
        Catalogue::parse(&bytes, &none()).unwrap_err(),
        Refused::NotJson {
            what: What::Catalogue,
            line: 1
        }
    );
}

#[test]
fn a_catalogue_with_an_unknown_field_or_a_missing_one_is_refused() {
    // An unknown field, at each level: the refusal says which object holds it, and
    // not its name.
    assert_eq!(
        refusal_after(|v| v["mirror-of-everything"] = json!(1)),
        value_refused("", Rule::Unknown)
    );
    assert_eq!(
        refusal_after(|v| v["models"][0]["rank"] = json!(1)),
        value_refused("models[0]", Rule::Unknown)
    );
    assert_eq!(
        refusal_after(|v| v["models"][0]["files"][0]["url"] = json!("x")),
        value_refused("models[0].files[0]", Rule::Unknown)
    );
    // A missing field, each of them.
    for field in ["format", "models"] {
        assert_eq!(
            refusal_after(|v| {
                v.as_object_mut().unwrap().remove(field);
            }),
            value_refused(field, Rule::Missing)
        );
    }
    for field in [
        "name", "revision", "adapter", "licence", "features", "origins", "files",
    ] {
        assert_eq!(
            refusal_after(|v| {
                v["models"][0].as_object_mut().unwrap().remove(field);
            }),
            value_refused(&format!("models[0].{field}"), Rule::Missing)
        );
    }
    for field in ["name", "size", "sha256", "from"] {
        assert_eq!(
            refusal_after(|v| {
                v["models"][0]["files"][1]
                    .as_object_mut()
                    .unwrap()
                    .remove(field);
            }),
            value_refused(&format!("models[0].files[1].{field}"), Rule::Missing)
        );
    }
}

#[test]
fn a_catalogue_with_a_value_of_another_kind_is_refused() {
    // A size as a text, as a fraction, below nothing and past what 64 bits hold.
    for size in [
        json!("3"),
        json!(3.5),
        json!(-3),
        json!(1e30),
        json!(null),
        json!([3]),
    ] {
        assert_eq!(
            refusal_after(|v| v["models"][0]["files"][0]["size"] = size),
            value_refused("models[0].files[0].size", Rule::Kind)
        );
    }
    assert_eq!(
        refusal_after(|v| v["models"][0]["name"] = json!(7)),
        value_refused("models[0].name", Rule::Kind)
    );
    assert_eq!(
        refusal_after(|v| v["format"] = json!(true)),
        value_refused("format", Rule::Kind)
    );
    assert_eq!(
        refusal_after(|v| v["models"][0]["features"] = json!(null)),
        value_refused("models[0].features", Rule::Kind)
    );
    assert_eq!(
        refusal_after(|v| v["models"][0]["features"][0] = json!(1)),
        value_refused("models[0].features[0]", Rule::Kind)
    );
    assert_eq!(
        refusal_after(|v| v["models"][0]["origins"][0] = json!({})),
        value_refused("models[0].origins[0]", Rule::Kind)
    );
    assert_eq!(
        refusal_after(|v| v["models"] = json!({})),
        value_refused("models", Rule::Kind)
    );
    assert_eq!(
        refusal_after(|v| v["models"][0] = json!("tiny")),
        value_refused("models[0]", Rule::Kind)
    );
    assert_eq!(
        refusal_after(|v| v["models"][0]["files"][1] = json!([])),
        value_refused("models[0].files[1]", Rule::Kind)
    );
    // The catalogue itself is an object with its fields by name: its values in a list,
    // in the order of the fields, are no catalogue.
    for text in ["[]", "null", "7", "\"catalogue/2\"", "[\"catalogue/2\",[]]"] {
        assert_eq!(
            Catalogue::parse(text.as_bytes(), &none()).unwrap_err(),
            value_refused("", Rule::Kind),
            "{text}"
        );
    }
}

#[test]
fn a_catalogue_with_a_field_given_twice_is_refused_by_its_line() {
    // The second is not taken in silence, and neither is the first.
    let whole = written().to_string();
    let twice = whole.replace("\"name\":\"tiny\"", "\"name\":\"tiny\",\"name\":\"tiny\"");
    assert_ne!(twice, whole);
    let refused = Catalogue::parse(twice.as_bytes(), &none()).unwrap_err();
    assert_eq!(
        refused,
        Refused::Twice {
            what: What::Catalogue,
            line: 1
        }
    );
    shows_nothing_of(&refused, &["name", "tiny"]);
    // On the line it is on; and at any depth, in an object no field asks for.
    let lower = whole.replacen(
        "\"models\":[{",
        "\"models\":[\n\n{\"extra\":{\"a\":1,\"a\":2},",
        1,
    );
    assert_ne!(lower, whole);
    assert_eq!(
        Catalogue::parse(lower.as_bytes(), &none()).unwrap_err(),
        Refused::Twice {
            what: What::Catalogue,
            line: 3
        }
    );
    // A key spelled two ways is one key.
    let spelled = whole.replace(
        "\"name\":\"tiny\"",
        "\"name\":\"tiny\",\"n\\u0061me\":\"tiny\"",
    );
    assert_eq!(
        Catalogue::parse(spelled.as_bytes(), &none()).unwrap_err(),
        Refused::Twice {
            what: What::Catalogue,
            line: 1
        }
    );
}

#[test]
fn a_catalogue_of_another_format_is_refused() {
    for format in [
        "catalogue/1",
        "catalogue/3",
        "catalogue/2 ",
        "installation/1",
        "",
    ] {
        assert_eq!(
            refusal_after(|v| v["format"] = json!(format)),
            value_refused("format", Rule::Format)
        );
    }
}

#[test]
fn a_hash_of_the_wrong_length_is_refused() {
    let good = hash(b"abc").to_string();
    let upper = good.to_uppercase().replace("SHA256:", "sha256:");
    for sha256 in [
        good[..good.len() - 1].to_owned(),
        format!("{good}0"),
        good.trim_start_matches("sha256:").to_owned(),
        good.replace("sha256:", "sha512:"),
        upper,
        format!("{}g", &good[..good.len() - 1]),
        format!(" {good}"),
        String::new(),
    ] {
        let refused = refusal_after(|v| v["models"][0]["files"][1]["sha256"] = json!(sha256));
        assert_eq!(
            refused,
            value_refused("models[0].files[1].sha256", Rule::Hash),
            "{sha256:?}"
        );
        if sha256.len() > 8 {
            shows_nothing_of(&refused, &[&sha256]);
        }
    }
}

#[test]
fn a_file_name_that_is_not_one_plain_name_is_refused() {
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
        "gr\u{e4}ph.onnx",
    ] {
        let refused = refusal_after(|v| v["models"][0]["files"][0]["name"] = json!(name));
        assert_eq!(
            refused,
            value_refused("models[0].files[0].name", Rule::FileName),
            "{name:?}"
        );
        if name.len() > 3 {
            shows_nothing_of(&refused, &[name]);
        }
    }
    // And a model's name that is not one plain name in lower case.
    for name in ["", "..", "Tiny", "models/tiny", "tiny/..", ".tiny"] {
        assert_eq!(
            refusal_after(|v| v["models"][0]["name"] = json!(name)),
            value_refused("models[0].name", Rule::ModelName),
            "{name:?}"
        );
    }
    assert_eq!(
        refusal_after(|v| v["models"][0]["adapter"] = json!("../adapter")),
        value_refused("models[0].adapter", Rule::ModelName)
    );
}

#[test]
fn a_size_of_zero_is_refused_and_so_is_one_past_the_most_a_file_may_be() {
    assert_eq!(FILE_MAX_BYTES, 1 << 40);
    assert_eq!(
        refusal_after(|v| v["models"][0]["files"][0]["size"] = json!(0)),
        value_refused("models[0].files[0].size", Rule::Size)
    );
    assert_eq!(
        refusal_after(|v| v["models"][0]["files"][0]["size"] = json!(FILE_MAX_BYTES + 1)),
        value_refused("models[0].files[0].size", Rule::Size)
    );
    for size in [1, FILE_MAX_BYTES] {
        let mut value = written();
        value["models"][0]["files"][0]["size"] = json!(size);
        assert_eq!(
            read(&value).unwrap().entry("tiny").unwrap().files()[1].size(),
            size
        );
    }
    // The same rule where a file is made, not read.
    let from = Address::of("https://weights.example.org/f", &none()).unwrap();
    for size in [0, FILE_MAX_BYTES + 1] {
        assert_eq!(
            CatalogueFile::new(file_name("f"), size, hash(b"f"), from.clone(), &none())
                .unwrap_err(),
            value_refused("", Rule::Size)
        );
    }
}

#[test]
fn every_other_field_of_an_entry_keeps_its_rule() {
    assert_eq!(
        refusal_after(|v| v["models"][0]["revision"] = json!("1 2")),
        value_refused("models[0].revision", Rule::Revision)
    );
    assert_eq!(
        refusal_after(|v| v["models"][0]["licence"] = json!("MIT\nApache")),
        value_refused("models[0].licence", Rule::Licence)
    );
    for feature in ["Phrase", "", "a b", "9lives"] {
        assert_eq!(
            refusal_after(|v| v["models"][0]["features"][1] = json!(feature)),
            value_refused("models[0].features[1]", Rule::Feature)
        );
    }
    // An origin is an origin (ADR 0012), and no more: a path's worth is refused with a
    // query, a user is refused, and another scheme.
    for (text, why) in [
        ("ftp://weights.example.org", NotAllowed::Scheme),
        ("https://weights.example.org/?x=1", NotAllowed::Query),
        ("weights.example.org", NotAllowed::Scheme),
        ("https://127.1", NotAllowed::Host),
    ] {
        assert_eq!(
            refusal_after(|v| v["models"][0]["origins"][0] = json!(text)),
            value_refused("models[0].origins[0]", Rule::Origin(why)),
            "{text}"
        );
    }
    // An origin is written as an origin: with a path it is an address, and is refused.
    assert_eq!(
        refusal_after(|v| v["models"][0]["origins"][0] = json!("https://weights.example.org/files")),
        value_refused("models[0].origins[0]", Rule::OriginAlone)
    );
    let with_user = ["https://", "user", "@", "weights.example.org/f"].concat();
    for (text, why) in [
        (with_user.as_str(), NotAllowed::UserInfo),
        ("https://weights.example.org/f#part", NotAllowed::Fragment),
        ("file:///etc/passwd", NotAllowed::Scheme),
    ] {
        assert_eq!(
            refusal_after(|v| v["models"][0]["files"][0]["from"] = json!(text)),
            value_refused("models[0].files[0].from", Rule::Address(why)),
            "{text}"
        );
    }
}

// --- every list is bounded, and names nothing twice -------------------------------------------

#[test]
fn a_catalogue_holds_a_bounded_count_of_models_each_named_once() {
    assert_eq!(MODELS_LIMIT, 256);
    let entries = |count: usize| -> Vec<Entry> {
        (0..count)
            .map(|at| entry(&format!("model-{at:03}")))
            .collect()
    };
    assert!(Catalogue::new(entries(MODELS_LIMIT), &none()).is_ok());
    assert_eq!(
        Catalogue::new(entries(MODELS_LIMIT + 1), &none()).unwrap_err(),
        value_refused("models", Rule::Count { most: MODELS_LIMIT })
    );
    assert_eq!(
        Catalogue::new(vec![entry("tiny"), entry("sam3"), entry("tiny")], &none()).unwrap_err(),
        value_refused("models", Rule::NamedTwice)
    );
    // Read, the count is looked at before any model is: 257 models that are not models.
    let mut value = written();
    value["models"] = Value::Array(vec![value["models"][0].clone(); MODELS_LIMIT + 1]);
    assert_eq!(
        read(&value).unwrap_err(),
        value_refused("models", Rule::Count { most: MODELS_LIMIT })
    );
    let mut value = written();
    let first = value["models"][0].clone();
    value["models"].as_array_mut().unwrap().push(first);
    assert_eq!(
        read(&value).unwrap_err(),
        value_refused("models", Rule::NamedTwice)
    );
}

#[test]
fn an_entry_holds_one_to_a_bounded_count_of_files_each_named_once_in_either_case() {
    assert_eq!(FILES_LIMIT, 64);
    let with_files = |count: usize| {
        let mut parts = parts("tiny");
        parts.files = (0..count)
            .map(|at| file(&format!("part-{at:02}.bin"), 1, b"x"))
            .collect();
        Entry::new(parts, &none())
    };
    assert!(with_files(1).is_ok());
    assert!(with_files(FILES_LIMIT).is_ok());
    assert_eq!(
        with_files(0).unwrap_err(),
        value_refused("files", Rule::NoFiles)
    );
    assert_eq!(
        with_files(FILES_LIMIT + 1).unwrap_err(),
        value_refused("files", Rule::Count { most: FILES_LIMIT })
    );
    // Two files of one name; and of one name but for the case of a letter, which are
    // one name on a disk that does not tell the cases apart.
    for (first, second) in [("graph.onnx", "graph.onnx"), ("graph.onnx", "Graph.ONNX")] {
        let mut parts = parts("tiny");
        parts.files = vec![file(first, 1, b"x"), file(second, 2, b"y")];
        assert_eq!(
            Entry::new(parts, &none()).unwrap_err(),
            value_refused("files", Rule::NamedTwice)
        );
    }
    // Read: the refusal says which model.
    assert_eq!(
        refusal_after(|v| v["models"][0]["files"] = json!([])),
        value_refused("models[0].files", Rule::NoFiles)
    );
    assert_eq!(
        refusal_after(|v| {
            let one = v["models"][0]["files"][0].clone();
            v["models"][0]["files"] = Value::Array(vec![one; FILES_LIMIT + 1]);
        }),
        value_refused("models[0].files", Rule::Count { most: FILES_LIMIT })
    );
    assert_eq!(
        refusal_after(|v| v["models"][0]["files"][1]["name"] = json!("GRAPH.onnx")),
        value_refused("models[0].files", Rule::NamedTwice)
    );
}

#[test]
fn an_entry_holds_a_bounded_count_of_features_each_a_name_declared_once() {
    assert_eq!(FEATURES_LIMIT, 16);
    let with_features = |features: Vec<String>| {
        let mut parts = parts("tiny");
        parts.features = features;
        Entry::new(parts, &none())
    };
    let many = |count: usize| (0..count).map(|at| format!("feature-{at}")).collect();
    assert!(with_features(Vec::new()).is_ok());
    assert!(with_features(many(FEATURES_LIMIT)).is_ok());
    assert_eq!(
        with_features(many(FEATURES_LIMIT + 1)).unwrap_err(),
        value_refused(
            "features",
            Rule::Count {
                most: FEATURES_LIMIT
            }
        )
    );
    assert_eq!(
        with_features(vec!["box".to_owned(), "box".to_owned()]).unwrap_err(),
        value_refused("features", Rule::NamedTwice)
    );
    assert_eq!(
        with_features(vec!["box".to_owned(), "Not A Name".to_owned()]).unwrap_err(),
        value_refused("features[1]", Rule::Feature)
    );
    assert_eq!(
        refusal_after(|v| v["models"][0]["features"] = json!(["box", "phrase", "box"])),
        value_refused("models[0].features", Rule::NamedTwice)
    );
}

#[test]
fn an_entry_names_a_bounded_count_of_origins_once_each_and_every_file_comes_from_one_of_them() {
    assert_eq!(ORIGINS_LIMIT, 8);
    let with_origins = |origins: Vec<whizbang_core::egress::Origin>| {
        let mut parts = parts("tiny");
        parts.origins = origins;
        Entry::new(parts, &none())
    };
    let mut many: Vec<_> = (0..ORIGINS_LIMIT - 1)
        .map(|at| origin(&format!("https://mirror-{at}.example.net")))
        .collect();
    many.push(origin("https://weights.example.org"));
    let full = with_origins(many.clone()).unwrap();
    // Kept in the order of their text, whatever order they were given in.
    let mut reversed = many.clone();
    reversed.reverse();
    assert_eq!(with_origins(reversed).unwrap(), full);
    assert_eq!(full.origins()[0], origin("https://mirror-0.example.net"));
    assert!(full.may_download_from(&origin("https://mirror-3.example.net")));
    assert!(!full.may_download_from(&origin("http://mirror-3.example.net")));
    assert!(!full.may_download_from(&origin("https://mirror-3.example.net:8443")));
    many.push(origin("https://one-more.example.net"));
    assert_eq!(
        with_origins(many).unwrap_err(),
        value_refused(
            "origins",
            Rule::Count {
                most: ORIGINS_LIMIT
            }
        )
    );
    assert_eq!(
        with_origins(vec![
            origin("https://weights.example.org"),
            origin("https://WEIGHTS.example.org:443")
        ])
        .unwrap_err(),
        value_refused("origins", Rule::NamedTwice)
    );
    // A file whose address is at an origin the entry does not name: by another host,
    // another scheme, another port, and with no origin named at all.
    assert_eq!(
        with_origins(Vec::new()).unwrap_err(),
        value_refused("files[0].from", Rule::Elsewhere)
    );
    for from in [
        "https://elsewhere.example.org/files/graph.onnx",
        "http://weights.example.org/files/graph.onnx",
        "https://weights.example.org:8443/files/graph.onnx",
    ] {
        assert_eq!(
            refusal_after(|v| v["models"][0]["files"][0]["from"] = json!(from)),
            value_refused("models[0].files[0].from", Rule::Elsewhere),
            "{from}"
        );
    }
}

/// One change to a record as a value.
type Change = Box<dyn Fn(&mut Value)>;

// --- a secret is never read into a name, and never written -----------------------------------

#[test]
fn a_catalogue_that_holds_a_secret_anywhere_is_refused_whole_and_not_shown() {
    let with_marker = format!("tiny-{MARKER}");
    let changes: Vec<Change> = vec![
        // A model's name and a file's, each a name but for the secret in it.
        Box::new(|v| v["models"][0]["name"] = json!(format!("tiny-{MARKER}"))),
        Box::new(|v| v["models"][0]["files"][0]["name"] = json!(format!("{MARKER}.onnx"))),
        // Every other text, and a key.
        Box::new(|v| v["models"][0]["revision"] = json!(MARKER)),
        Box::new(|v| v["models"][0]["adapter"] = json!(MARKER)),
        Box::new(|v| v["models"][0]["licence"] = json!(format!("under {MARKER}"))),
        Box::new(|v| v["models"][0]["features"][0] = json!(MARKER)),
        Box::new(|v| v["models"][0]["origins"][0] = json!(format!("https://{MARKER}.example.org"))),
        Box::new(|v| {
            v["models"][0]["files"][0]["from"] =
                json!(format!("https://weights.example.org/f?key={MARKER}"))
        }),
        Box::new(|v| v["format"] = json!(MARKER)),
        Box::new(|v| v[MARKER] = json!(1)),
        Box::new(|v| v["models"][0]["files"][0]["sha256"] = json!(MARKER)),
    ];
    for (at, change) in changes.iter().enumerate() {
        let mut value = written();
        change(&mut value);
        let text = value.to_string();
        assert!(text.contains(MARKER), "case {at}");
        // As it is written, and with its first character spelled as JSON may spell it,
        // which the bytes of the file do not hold.
        let spelled = text.replace(MARKER, &format!("\\u0073{}", &MARKER[1..]));
        assert!(!spelled.contains(MARKER));
        for text in [&text, &spelled] {
            let refused = Catalogue::parse(text.as_bytes(), &secrets()).unwrap_err();
            assert_eq!(refused, Refused::HoldsSecret, "case {at}: {text}");
            shows_nothing_of(&refused, &[MARKER, &MARKER[..4], &with_marker]);
        }
    }
    // In a comment's place, where no field is: the file is looked at whole.
    let text = format!("{} \t{MARKER}", written());
    assert_eq!(
        Catalogue::parse(text.as_bytes(), &secrets()).unwrap_err(),
        Refused::HoldsSecret
    );
    // Told of no secret, the first of these is a catalogue: the secret is what refuses.
    let mut value = written();
    changes[0](&mut value);
    assert!(read(&value).is_ok());
}

#[test]
fn a_secret_that_writing_the_catalogue_would_spell_is_refused_where_the_catalogue_is_made() {
    // The address is written with its port left out, which brings its host and its
    // path together: `example.org/files` is in what would be written and not in what
    // was read.
    let told = whizbang_core::contract::Secrets::new(["example.org/files".to_owned()]).unwrap();
    let mut value = written();
    for at in [0, 1] {
        let from = value["models"][0]["files"][at]["from"]
            .as_str()
            .unwrap()
            .to_owned();
        value["models"][0]["files"][at]["from"] =
            json!(from.replace("example.org/", "example.org:443/"));
    }
    let text = value.to_string();
    assert!(!text.contains("example.org/files"));
    assert!(read(&value).is_ok());
    let refused = Catalogue::parse(text.as_bytes(), &told).unwrap_err();
    assert_eq!(refused, Refused::HoldsSecret);
    // Two texts that are apart as a person wrote them and side by side as whizbang
    // writes them: a secret that only the written form spells is refused too.
    let spelled =
        whizbang_core::contract::Secrets::new(
            ["\"adapter\":\"owlv2-sam2\",\"licence\"".to_owned()],
        )
        .unwrap();
    let pretty = serde_json::to_string_pretty(&written()).unwrap();
    assert_eq!(
        Catalogue::parse(pretty.as_bytes(), &spelled).unwrap_err(),
        Refused::HoldsSecret
    );
    assert_eq!(
        Catalogue::new(vec![entry("tiny")], &spelled).unwrap_err(),
        Refused::HoldsSecret
    );
}

#[test]
fn a_part_that_holds_a_secret_is_refused_where_the_part_is_made() {
    // A file whose address, written again without its port, spells a secret: refused
    // by the file's own constructor, before any catalogue is made of it. The address
    // was made by one that was told of no secret; told of this one, the core refuses
    // the address itself.
    let told = whizbang_core::contract::Secrets::new(["example.org/files".to_owned()]).unwrap();
    let text = "https://weights.example.org:443/files/f";
    assert_eq!(Address::of(text, &told).unwrap_err(), NotAllowed::Secret);
    let from = Address::of(text, &none()).unwrap();
    assert_eq!(
        CatalogueFile::new(file_name("f"), 1, hash(b"f"), from.clone(), &told).unwrap_err(),
        Refused::HoldsSecret
    );
    assert!(CatalogueFile::new(file_name("f"), 1, hash(b"f"), from, &none()).is_ok());
    // A feature that is a name and holds a secret: refused by the entry's.
    let mut with_feature = parts("tiny");
    with_feature.features = vec![MARKER.to_owned()];
    assert_eq!(
        Entry::new(with_feature.clone(), &secrets()).unwrap_err(),
        Refused::HoldsSecret
    );
    assert!(Entry::new(with_feature, &none()).is_ok());
    // A secret that only the lowering of a host makes (ADR 0012) is in no text of the
    // file as it was written: it is refused as a secret, not as a host that is none.
    // In an origin, and in the address of a file.
    let lowered = whizbang_core::contract::Secrets::new(["weights.example".to_owned()]).unwrap();
    let upper = written()
        .to_string()
        .replace("weights.example.org", "WEIGHTS.EXAMPLE.org");
    assert!(!upper.contains("weights.example"));
    assert_eq!(
        Catalogue::parse(upper.as_bytes(), &none()).unwrap(),
        read(&written()).unwrap()
    );
    assert_eq!(
        Catalogue::parse(upper.as_bytes(), &lowered).unwrap_err(),
        Refused::HoldsSecret
    );
    let lowered = whizbang_core::contract::Secrets::new(["elsewhere.example".to_owned()]).unwrap();
    let mut value = written();
    value["models"][0]["files"][0]["from"] = json!("https://ELSEWHERE.EXAMPLE.org/f");
    assert_eq!(
        read(&value).unwrap_err(),
        value_refused("models[0].files[0].from", Rule::Elsewhere)
    );
    assert_eq!(
        Catalogue::parse(value.to_string().as_bytes(), &lowered).unwrap_err(),
        Refused::HoldsSecret
    );
}

#[test]
fn what_is_printed_of_the_parts_of_an_entry_is_counts_and_never_a_feature() {
    // The parts are what an entry is made from, before the entry's constructor has
    // looked at a feature for a secret: what `{:?}` prints of them holds no feature,
    // no origin and no address.
    let mut parts = parts("tiny");
    parts.features = vec![MARKER.to_owned(), "box".to_owned()];
    let printed = format!("{parts:?}");
    assert_eq!(
        printed,
        "EntryParts { name: ModelName(tiny), title: Title(Tiny, for tests), \
         revision: Revision(2026-11.1), adapter: ModelName(owlv2-sam2), \
         licence: Licence(Apache-2.0), platforms: 2, features: 2, origins: 1, files: 2 }"
    );
    assert!(!printed.contains(MARKER) && !printed.contains("example.org"));
}

#[test]
fn a_list_over_its_bound_is_refused_for_its_count_before_any_of_its_items_is_looked_at() {
    // Each list one over its bound, and holding an item that would be refused for
    // itself: the refusal is the count's, so no item was looked at.
    let over = |list: &mut Value, most: usize, bad: Value| {
        let one = list[0].clone();
        let mut items = vec![one; most + 1];
        items[0] = bad;
        *list = Value::Array(items);
    };
    assert_eq!(
        refusal_after(|v| over(&mut v["models"], MODELS_LIMIT, json!("not a model"))),
        value_refused("models", Rule::Count { most: MODELS_LIMIT })
    );
    assert_eq!(
        refusal_after(|v| over(&mut v["models"][0]["files"], FILES_LIMIT, json!(7))),
        value_refused("models[0].files", Rule::Count { most: FILES_LIMIT })
    );
    assert_eq!(
        refusal_after(|v| over(&mut v["models"][0]["features"], FEATURES_LIMIT, json!(7))),
        value_refused(
            "models[0].features",
            Rule::Count {
                most: FEATURES_LIMIT
            }
        )
    );
    assert_eq!(
        refusal_after(|v| over(
            &mut v["models"][0]["origins"],
            ORIGINS_LIMIT,
            json!("ftp://x")
        )),
        value_refused(
            "models[0].origins",
            Rule::Count {
                most: ORIGINS_LIMIT
            }
        )
    );
}

#[test]
fn a_catalogue_too_long_to_be_read_back_is_not_made() {
    // 256 models of 64 files, each with a long address: more than a megabyte written.
    let long = "p".repeat(1900);
    let entries: Vec<Entry> = (0..40)
        .map(|at| {
            let mut parts = parts(&format!("model-{at:03}"));
            parts.files = (0..FILES_LIMIT)
                .map(|n| {
                    let from =
                        Address::of(&format!("https://weights.example.org/{long}/{n}"), &none())
                            .unwrap();
                    CatalogueFile::new(file_name(&format!("f{n}")), 1, hash(b"x"), from, &none())
                        .unwrap()
                })
                .collect();
            Entry::new(parts, &none()).unwrap()
        })
        .collect();
    assert_eq!(
        Catalogue::new(entries, &none()).unwrap_err(),
        Refused::TooLarge {
            what: What::Catalogue,
            limit: CATALOGUE_MAX_BYTES
        }
    );
}
