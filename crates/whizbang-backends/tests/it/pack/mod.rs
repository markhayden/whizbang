//! The pack (`SPEC.md` section 4; plan 1C, the protocol and worker strand): the
//! protocol's codec, the conformance suite and the worker's supervisor. So far the
//! codec ([`proto`]) and the folder of the fixed vectors: each task adds its module
//! here.

use std::path::Path;

mod proto;

/// The folder of the conformance suite's fixed vectors, from this crate's manifest.
fn vectors() -> &'static Path {
    Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../pack/conformance/vectors"
    ))
}

/// The most files the vectors folder is listed for: the loop stops there, whatever the
/// folder holds.
const FILES_MOST: usize = 256;

#[test]
fn the_vectors_folder_holds_its_readme_and_one_file_for_each_vector_and_nothing_else() {
    let listing = std::fs::read_dir(vectors());
    assert!(listing.is_ok(), "the folder is not there: {listing:?}");
    let mut names: Vec<String> = Vec::new();
    for entry in listing.unwrap().take(FILES_MOST) {
        let entry = entry.unwrap();
        assert!(entry.file_type().unwrap().is_file(), "{:?}", entry.path());
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    assert!(names.len() < FILES_MOST, "the folder was not listed whole");
    names.sort();
    // One file says what the folder holds, and how a vector is laid out.
    let readme = names.iter().position(|name| name == "README.md");
    assert!(readme.is_some(), "{names:?}");
    names.remove(readme.unwrap());
    let said = std::fs::read_to_string(vectors().join("README.md")).unwrap();
    for word in [
        "conformance suite",
        "pack/1",
        "0020",
        "frame",
        "refused",
        "open",
    ] {
        assert!(said.contains(word), "the README does not say {word}");
    }
    // Every other file is a vector: a plain name in lower case, and `.json`.
    assert!(names.len() >= 28, "{} vectors", names.len());
    for name in &names {
        let stem = name.strip_suffix(".json");
        let plain = |stem: &str| {
            let letter = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-';
            !stem.is_empty() && stem.bytes().all(letter)
        };
        assert!(stem.is_some_and(plain), "{name} is not a vector's name");
    }
}
