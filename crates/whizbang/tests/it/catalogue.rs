//! The catalogue this build ships, and its table of adapters (plan 1C, task N5; ADR
//! 0017, "The shipped catalogue"). Read as the binary reads it; nothing here asks
//! anything of any address.

use std::path::Path;

use whizbang::wiring;
use whizbang_backends::models::{Entry, Platform};

/// The one entry of the shipped catalogue.
fn sam3() -> Entry {
    let catalogue = wiring::catalogue().expect("the shipped catalogue is read");
    catalogue.entry("sam3").expect("it holds sam3").clone()
}

#[test]
fn the_shipped_catalogue_parses_names_sam3_and_no_fake_model() {
    let catalogue = wiring::catalogue().expect("the shipped catalogue is read");
    let names: Vec<&str> = catalogue
        .entries()
        .iter()
        .map(|entry| entry.name().as_str())
        .collect();
    assert_eq!(names, ["sam3"]);
    let entry = sam3();
    assert_eq!(entry.title().as_str(), "SAM 3");
    assert_eq!(entry.adapter().as_str(), "sam3");
    assert_eq!(entry.licence().as_str(), "SAM License");
    assert_eq!(entry.platforms(), [Platform::MacosArm64]);
    assert_eq!(entry.features().collect::<Vec<_>>(), ["phrase"]);
    assert_eq!(
        entry.needs_token().map(ToString::to_string).as_deref(),
        Some("https://huggingface.co")
    );
    // The files of D11: the weights as safetensors, and the small text files the
    // runtime reads beside them, with the licence.
    let files: Vec<&str> = entry.files().iter().map(|f| f.name().as_str()).collect();
    assert_eq!(
        files,
        [
            "LICENSE",
            "config.json",
            "merges.txt",
            "model.safetensors",
            "processor_config.json",
            "special_tokens_map.json",
            "tokenizer.json",
            "tokenizer_config.json",
            "vocab.json",
        ]
    );
    // The weights are the size the Hub's listing of the pinned revision states.
    let weights = entry
        .files()
        .iter()
        .find(|f| f.name().as_str() == "model.safetensors");
    assert_eq!(weights.map(|f| f.size()), Some(3_439_938_512));
    // No fake model, and nothing named as one of the test kit's or the pack's fakes.
    let written = String::from_utf8(catalogue.to_bytes()).unwrap();
    assert!(!written.contains("fake") && !written.contains("example"));
    // What is written of it reads back as the same catalogue.
    let again = whizbang_backends::models::Catalogue::parse(
        &catalogue.to_bytes(),
        &whizbang_core::contract::Secrets::none(),
    );
    assert_eq!(again.unwrap(), catalogue);
    assert!(wiring::SHIPPED_CATALOGUE.contains("\"format\": \"catalogue/2\""));
}

#[test]
fn every_file_of_the_shipped_entry_is_at_an_origin_the_entry_names_and_at_one_pinned_revision() {
    let entry = sam3();
    let origins: Vec<String> = entry.origins().iter().map(ToString::to_string).collect();
    // The Hub, and the host it moves the large file's download to (open question 2,
    // named to the owner and approved with the Hub's storage on 2026-11-16).
    assert_eq!(
        origins,
        ["https://huggingface.co", "https://us.aws.cdn.hf.co"]
    );
    let mut revisions = Vec::new();
    for file in entry.files() {
        let from = file.from();
        assert!(entry.may_download_from(from.origin()));
        assert_eq!(from.origin().to_string(), "https://huggingface.co");
        let rest = from.rest();
        let pinned = rest
            .strip_prefix("/facebook/sam3/resolve/")
            .unwrap_or_else(|| panic!("{rest}"));
        let (revision, name) = pinned.split_once('/').unwrap();
        // A commit of the repository: forty lower-case hex digits, never a branch.
        assert_eq!(revision.len(), 40, "{revision}");
        assert!(
            revision
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        );
        assert_eq!(name, file.name().as_str());
        revisions.push(revision.to_owned());
    }
    revisions.dedup();
    assert_eq!(revisions, ["3c879f39826c281e95690f02c7821c4de09afae7"]);
}

#[test]
fn the_shipped_entry_names_no_pickle() {
    // `sam3.pt` is a pickle: it is never downloaded or loaded (D11). Nor is any
    // other file of a pickle's kind.
    for file in sam3().files() {
        let name = file.name().as_str().to_ascii_lowercase();
        for pickle in [".pt", ".pth", ".pkl", ".pickle", ".ckpt", ".bin"] {
            assert!(!name.ends_with(pickle), "{name}");
        }
        assert!(!file.from().rest().to_ascii_lowercase().contains("sam3.pt"));
    }
}

#[test]
fn every_adapter_the_binarys_table_names_as_shipped_is_an_adapter_the_pack_has() {
    // An adapter is the pack's when its module is in the pack and the worker names
    // it. One the table names as shipped must be; one it names as not yet shipped
    // must not be, so that the day the pack has it (task R2a) the table is changed
    // with it, and nothing calls a model ready while its load would be refused.
    let pack = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pack/src/whizbang_pack");
    let worker = std::fs::read_to_string(pack.join("worker.py")).unwrap();
    let table = wiring::adapters();
    assert!(!table.is_empty());
    for adapter in &table {
        let name = adapter.name.as_str();
        let module = pack.join(format!("{name}.py")).exists();
        let named = worker.contains(&format!("\"{name}\""));
        assert_eq!(
            (module && named, module || named),
            (adapter.shipped, adapter.shipped),
            "the pack and the table disagree on the adapter {name}"
        );
    }
    // Every model of the catalogue has its adapter in the table, so `get` can
    // install it under the revision it will be loaded under.
    let catalogue = wiring::catalogue().unwrap();
    for entry in catalogue.entries() {
        assert!(table.iter().any(|adapter| &adapter.name == entry.adapter()));
    }
}
