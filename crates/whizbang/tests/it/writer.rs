//! One way to a writer (ADR 0015; plan 1B, task I4): the binary opens a store to
//! write in one place, `wiring::opening`, which is the entry point that recovers a
//! store before it hands out the writer. This holds the binary's sources to that:
//! every file under `crates/whizbang/src`, where the four direct opens were (two in
//! `import.rs`, one in `rascal.rs`, one in `store.rs`).
//!
//! It does not hold tests or the test kit to it, and they may keep `Store::open`: a
//! test that builds a store by hand, or that needs a writer with no recovery, opens
//! one directly. The binary's own unit tests are among its sources and are held.

use std::path::Path;

/// Every source file of the binary and of its library, with its text.
fn sources() -> Vec<(String, String)> {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = Vec::new();
    for entry in std::fs::read_dir(&folder).unwrap() {
        let path = entry.unwrap().path();
        assert!(
            path.is_file(),
            "{}: a folder of sources is not read here",
            path.display()
        );
        let name = path.file_name().unwrap().to_str().unwrap().to_owned();
        found.push((name, std::fs::read_to_string(&path).unwrap()));
    }
    found.sort();
    found
}

/// The lines of a text that are code: not a comment's.
fn code(text: &str) -> impl Iterator<Item = &str> {
    text.lines()
        .filter(|line| !line.trim_start().starts_with("//"))
}

#[test]
fn the_binary_opens_a_store_to_write_in_one_place_and_through_the_entry_point() {
    let sources = sources();
    let names: Vec<&str> = sources.iter().map(|(name, _)| name.as_str()).collect();
    for expected in [
        "import.rs",
        "rascal.rs",
        "spend.rs",
        "store.rs",
        "wiring.rs",
    ] {
        assert!(names.contains(&expected), "{names:?}");
    }
    // The store's own opens are named `open`, `open_existing` and
    // `open_in_existing_parent`, and so are the entry point's: a line of code that
    // names one, called or not, holds `Store::open` or `Opening::open`.
    for (name, text) in &sources {
        for line in code(text) {
            // The store's own open is named nowhere: no writer without a recovery.
            let named = ["Store::open", "Store>::open", "Store ::"];
            assert!(
                !named.iter().any(|name| line.contains(name)),
                "{name} opens a store to write past the entry point: {line}"
            );
            // The store's type is named where a writer is held, and nowhere is it
            // brought in under another name.
            assert!(
                !line.contains("Store as ") && !line.contains("store::*"),
                "{name}: {line}"
            );
            // The entry point is opened in the wiring, and nowhere else.
            assert!(
                !line.contains("Opening::open") || name == "wiring.rs",
                "{name} opens a store past `wiring::opening`: {line}"
            );
        }
    }
    // And the wiring opens it, in the one function.
    let wiring = &sources
        .iter()
        .find(|(name, _)| name == "wiring.rs")
        .unwrap()
        .1;
    let opened = code(wiring).filter(|line| line.contains("Opening::open"));
    assert_eq!(
        opened.count(),
        2,
        "one for each way a command opens a store"
    );
    // Each writing command asks that function: the store's, the importer and the
    // one that runs an action. `spend` writes nothing yet; when it does, the lines
    // above hold it to the same way.
    for (name, asks) in [("store.rs", 1), ("import.rs", 2), ("rascal.rs", 1)] {
        let text = &sources.iter().find(|(file, _)| file == name).unwrap().1;
        let asked = code(text).filter(|line| line.contains("wiring::opening("));
        assert_eq!(asked.count(), asks, "{name}");
    }
}
