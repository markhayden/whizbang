//! The pack (`SPEC.md` section 4; plan 1C, the protocol and worker strand): the
//! protocol's codec, the conformance suite and the worker's supervisor. A root only, so
//! far, and the folder the fixed vectors will be in: each task adds its module here.

use std::path::Path;

/// The folder of the conformance suite's fixed vectors, from this crate's manifest.
fn vectors() -> &'static Path {
    Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../pack/conformance/vectors"
    ))
}

#[test]
fn the_vectors_folder_holds_one_file_of_one_line_that_says_what_it_holds() {
    let listing = std::fs::read_dir(vectors());
    assert!(listing.is_ok(), "the folder is not there: {listing:?}");
    let mut names: Vec<String> = Vec::new();
    // A handful at most: the loop stops well before it could run away on a folder
    // that is not the one meant.
    for entry in listing.unwrap().take(16) {
        names.push(entry.unwrap().file_name().to_string_lossy().into_owned());
    }
    assert_eq!(names, ["README.md"]);
    let said = std::fs::read_to_string(vectors().join("README.md")).unwrap();
    assert_eq!(said.matches('\n').count(), 1, "{said:?}");
    assert!(said.ends_with('\n'), "{said:?}");
    assert!(said.contains("vectors"), "{said:?}");
    assert!(said.contains("conformance suite"), "{said:?}");
    assert!(said.contains("None yet"), "{said:?}");
}
