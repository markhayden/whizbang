//! The identity of an installation (ADR 0017, "The identity of an installation"): the
//! SHA-256 of one encoding of its record, as ADR 0001 hashes every identity. The vectors
//! here were computed apart from the code, from the ADR's words.

use whizbang_backends::models::{InstallationId, InstallationRecord, InstalledFile, Refused, Rule};
use whizbang_core::identity::Digest;

use super::{MARKER, entry, file_name, hash, model, none, revision, secrets, shows_nothing_of};

fn installed(name: &str, size: u64, of: &[u8]) -> InstalledFile {
    InstalledFile::new(file_name(name), size, hash(of)).unwrap()
}

/// A record of these five texts and files.
fn record(
    name: &str,
    catalogue: &str,
    adapter: &str,
    adapter_revision: &str,
    files: Vec<InstalledFile>,
) -> InstallationRecord {
    InstallationRecord::new(
        model(name),
        revision(catalogue),
        model(adapter),
        revision(adapter_revision),
        files,
        &none(),
    )
    .unwrap()
}

fn two_files() -> Vec<InstalledFile> {
    vec![
        installed("graph.onnx", 3, b"abc"),
        installed("LICENSE.txt", 5, b"terms"),
    ]
}

fn tiny(adapter_revision: &str) -> InstallationRecord {
    record(
        "tiny",
        "2026-11.1",
        "owlv2-sam2",
        adapter_revision,
        two_files(),
    )
}

#[test]
fn the_identity_of_a_record_is_the_digest_the_adr_gives_for_it() {
    // The smallest record: the model `m`, the catalogue's revision `1`, the adapter
    // `a`, its revision `1`, and one file `f` of 1 byte whose content is `x`. The bytes
    // hashed, written out: the prefix and the kind and a zero byte; four texts, each
    // `b`, its length in eight bytes and its bytes; the count of files as `u` and
    // eight bytes; and for the file its name, its size and its hash.
    let encoding = [
        b"whizbang:installation/1\0".as_slice(),
        b"b\0\0\0\0\0\0\0\x01m",
        b"b\0\0\0\0\0\0\0\x011",
        b"b\0\0\0\0\0\0\0\x01a",
        b"b\0\0\0\0\0\0\0\x011",
        b"u\0\0\0\0\0\0\0\x01",
        b"b\0\0\0\0\0\0\0\x01f",
        b"u\0\0\0\0\0\0\0\x01",
        b"d",
        hash(b"x").as_bytes(),
    ]
    .concat();
    assert_eq!(encoding.len(), 125);
    let smallest = record("m", "1", "a", "1", vec![installed("f", 1, b"x")]);
    assert_eq!(smallest.identity().digest(), Digest::of_bytes(&encoding));
    // The same three digests, computed apart from this code and this test.
    assert_eq!(
        smallest.identity().to_string(),
        "sha256:9bdb763f4f3b18ce601572a1dce1142314a2cab53882897aebd70d357daf6a1f"
    );
    assert_eq!(
        tiny("7").identity().to_string(),
        "sha256:9d10eec43a1b6f8abc3225115d33d98d2469e7bfd9c68c2c5ff428b50b5dd772"
    );
    assert_eq!(
        tiny("8").identity().to_string(),
        "sha256:f04273f754765adcda8eac3dcfcc5f6d1a66bfe972276e0a8ac80897b9056ced"
    );
    // The record made from a catalogue's entry is that record, and has that identity.
    let from_entry = InstallationRecord::of(&entry("tiny"), revision("7"), &none()).unwrap();
    assert_eq!(from_entry.identity(), tiny("7").identity());
}

#[test]
fn two_records_that_differ_in_the_adapters_revision_alone_have_two_identities() {
    let (seven, eight) = (tiny("7"), tiny("8"));
    assert_eq!(seven.model(), eight.model());
    assert_eq!(seven.catalogue_revision(), eight.catalogue_revision());
    assert_eq!(seven.adapter(), eight.adapter());
    assert_eq!(seven.files(), eight.files());
    assert_ne!(seven.adapter_revision(), eight.adapter_revision());
    assert_ne!(seven.identity(), eight.identity());
    // And the same record, made twice, has one.
    assert_eq!(seven.identity(), tiny("7").identity());
}

#[test]
fn every_part_of_a_record_is_part_of_its_identity() {
    let base = tiny("7").identity();
    let others = [
        record("tinier", "2026-11.1", "owlv2-sam2", "7", two_files()),
        record("tiny", "2026-11.2", "owlv2-sam2", "7", two_files()),
        record("tiny", "2026-11.1", "owlv2-sam3", "7", two_files()),
        // A file's name, its size, its hash; one file fewer, one more.
        record(
            "tiny",
            "2026-11.1",
            "owlv2-sam2",
            "7",
            vec![
                installed("graph2.onnx", 3, b"abc"),
                installed("LICENSE.txt", 5, b"terms"),
            ],
        ),
        record(
            "tiny",
            "2026-11.1",
            "owlv2-sam2",
            "7",
            vec![
                installed("graph.onnx", 4, b"abc"),
                installed("LICENSE.txt", 5, b"terms"),
            ],
        ),
        record(
            "tiny",
            "2026-11.1",
            "owlv2-sam2",
            "7",
            vec![
                installed("graph.onnx", 3, b"abd"),
                installed("LICENSE.txt", 5, b"terms"),
            ],
        ),
        record(
            "tiny",
            "2026-11.1",
            "owlv2-sam2",
            "7",
            vec![installed("graph.onnx", 3, b"abc")],
        ),
        record(
            "tiny",
            "2026-11.1",
            "owlv2-sam2",
            "7",
            vec![
                installed("graph.onnx", 3, b"abc"),
                installed("LICENSE.txt", 5, b"terms"),
                installed("more.bin", 1, b"x"),
            ],
        ),
        // Which file has which size and hash.
        record(
            "tiny",
            "2026-11.1",
            "owlv2-sam2",
            "7",
            vec![
                installed("graph.onnx", 5, b"terms"),
                installed("LICENSE.txt", 3, b"abc"),
            ],
        ),
    ];
    let mut seen = vec![base];
    for other in &others {
        let identity = other.identity();
        assert!(!seen.contains(&identity), "{other:?}");
        seen.push(identity);
    }
    // Where one text ends and the next begins is part of it: each text is hashed with
    // its length, so moving a character from one to its neighbour is another identity.
    let split = [
        record("ab", "c", "d", "e", vec![installed("f", 1, b"x")]),
        record("a", "bc", "d", "e", vec![installed("f", 1, b"x")]),
        record("a", "b", "cd", "e", vec![installed("f", 1, b"x")]),
        record("a", "b", "c", "de", vec![installed("f", 1, b"x")]),
    ];
    for (at, one) in split.iter().enumerate() {
        for other in &split[at + 1..] {
            assert_ne!(one.identity(), other.identity());
        }
    }
}

#[test]
fn an_identity_does_not_depend_on_the_order_the_files_were_given_in() {
    let mut reversed = two_files();
    reversed.reverse();
    let other = record("tiny", "2026-11.1", "owlv2-sam2", "7", reversed);
    assert_eq!(other.identity(), tiny("7").identity());
    assert_eq!(other.to_bytes(), tiny("7").to_bytes());
}

#[test]
fn an_identity_read_back_from_a_record_s_bytes_is_the_identity_of_the_record_that_wrote_them() {
    let record = tiny("7");
    let again = InstallationRecord::parse(&record.to_bytes(), &none()).unwrap();
    assert_eq!(again.identity(), record.identity());
}

#[test]
fn an_identity_is_written_one_way_and_read_strictly() {
    let identity = tiny("7").identity();
    let text = identity.to_string();
    assert_eq!(text, format!("sha256:{}", identity.hex()));
    assert_eq!(identity.hex().len(), 64);
    assert_eq!(text, identity.digest().to_string());
    assert_eq!(format!("{identity:?}"), format!("InstallationId({text})"));
    assert_eq!(InstallationId::from_text(&text, &none()).unwrap(), identity);
    let upper = text.to_uppercase().replace("SHA256:", "sha256:");
    for other in [
        identity.hex(),
        format!("{text}\n"),
        format!(" {text}"),
        text[..text.len() - 1].to_owned(),
        format!("{text}0"),
        upper,
        text.replace("sha256:", "sha256:/"),
        "sha256:../../../../etc/passwd".to_owned(),
        String::new(),
    ] {
        let refused = InstallationId::from_text(&other, &none()).unwrap_err();
        assert_eq!(
            refused,
            Refused::Value {
                at: String::new(),
                rule: Rule::Hash
            },
            "{other:?}"
        );
        if other.len() > 8 {
            shows_nothing_of(&refused, &[&other]);
        }
    }
    // A text that holds a secret is refused for the secret, whatever else it is.
    let refused = InstallationId::from_text(&format!("sha256:{MARKER}"), &secrets()).unwrap_err();
    assert_eq!(refused, Refused::HoldsSecret);
}
