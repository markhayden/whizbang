//! The model tier's committed evidence (plan 1C, task R2b; decision D15), held in the fast
//! tier: the record `scripts/model-evidence.sh` wrote is of the tree's adapter, lock and
//! fixture, and the fixture picture, which is public, holds no metadata.

use std::path::{Path, PathBuf};

use whizbang::wiring;
use whizbang_core::identity::{DecodeLimits, Digest, identify};

use crate::model_tier::FIXTURE;

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn digest_of(path: &str) -> String {
    let bytes = std::fs::read(repository().join(path)).unwrap();
    Digest::of_bytes(&bytes).to_string()
}

/// Fields of the numbers file, `name` and the value without its quotes, in order.
type Fields = Vec<(String, String)>;

/// The fields of the numbers file before its first table, as `name = value`, each value
/// without its quotes; and the tables `[[round]]`, each the same.
fn record() -> (Fields, Vec<Fields>) {
    let text = std::fs::read_to_string(repository().join("docs/results/m1c-local-pack.toml"))
        .expect("the record of the model tier is committed");
    let mut top = Vec::new();
    let mut rounds: Vec<Fields> = Vec::new();
    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        if line == "[[round]]" {
            rounds.push(Vec::new());
            continue;
        }
        let (name, value) = line.split_once(" = ").expect("a line is name = value");
        let field = (name.to_owned(), value.trim_matches('"').to_owned());
        match rounds.last_mut() {
            Some(round) => round.push(field),
            None => top.push(field),
        }
    }
    (top, rounds)
}

fn field<'a>(fields: &'a [(String, String)], name: &str) -> &'a str {
    fields
        .iter()
        .find(|(given, _)| given == name)
        .map(|(_, value)| value.as_str())
        .unwrap_or_else(|| panic!("the record has no {name}"))
}

#[test]
fn the_committed_record_names_the_adapters_revision_and_the_lock_the_tree_has() {
    let (top, rounds) = record();
    // Of this tree: the lock, the adapter's source and the fixture, by their digests; the
    // adapter's revision in the binary's table. A change to any of them makes the record
    // stale, and `scripts/model-evidence.sh` is run again on the Mac.
    assert_eq!(field(&top, "lock"), digest_of("pack/uv.lock"));
    assert_eq!(
        field(&top, "adapter_source"),
        digest_of("pack/src/whizbang_pack/sam3.py")
    );
    assert_eq!(
        field(&top, "fixture"),
        digest_of("docs/results/m1c-local-pack/circle.png")
    );
    let table = wiring::adapters();
    let sam3 = table
        .iter()
        .find(|adapter| adapter.name.as_str() == "sam3")
        .unwrap();
    assert!(sam3.shipped);
    assert_eq!(field(&top, "adapter"), "sam3");
    assert_eq!(field(&top, "adapter_revision"), sam3.revision.as_str());
    // What it shows: the GPU, with the fallback off, nothing contacted, in three runs
    // each with an instance and one load for two requests.
    assert_eq!(field(&top, "device"), "mps");
    assert_eq!(field(&top, "fallback"), "off");
    assert_eq!(field(&top, "hosts_contacted"), "none");
    assert_eq!(rounds.len(), 3);
    for round in &rounds {
        assert_eq!(field(round, "phrase_device"), "mps");
        assert_ne!(field(round, "phrase_instances"), "0");
        assert_eq!(field(round, "loads"), "1");
        assert_eq!(field(round, "still_running_after"), "0");
    }
    // The page beside it says the same lock and the same adapter.
    let page =
        std::fs::read_to_string(repository().join("docs/results/m1c-local-pack.md")).unwrap();
    for value in [
        field(&top, "lock"),
        field(&top, "adapter_source"),
        field(&top, "installation"),
    ] {
        assert!(page.contains(value), "the page does not name {value}");
    }
    assert!(page.contains(&format!("revision `{}`", sam3.revision.as_str())));
}

#[test]
fn the_fixture_picture_holds_no_metadata() {
    // A PNG of the signature and chunks: the header, the pixels and the end, and nothing
    // else: no EXIF (`eXIf`), no colour profile (`iCCP`, `sRGB`, `gAMA`, `cHRM`), no text
    // (`tEXt`, `zTXt`, `iTXt`), no time and no physical size.
    let bytes = FIXTURE;
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    let mut kinds = Vec::new();
    let mut at = 8;
    while at < bytes.len() {
        let length = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        kinds.push(String::from_utf8(bytes[at + 4..at + 8].to_vec()).unwrap());
        at += 12 + length;
    }
    assert_eq!(at, bytes.len());
    kinds.dedup();
    assert_eq!(kinds, ["IHDR", "IDAT", "IEND"]);
    // And it decodes as the test uses it: 640 by 480, no alpha, no profile.
    let found = identify(bytes, &DecodeLimits::default()).unwrap();
    assert_eq!((found.stored.width, found.stored.height), (640, 480));
    assert!(!found.stored.alpha);
    assert_eq!(found.stored.orientation, 1);
}
