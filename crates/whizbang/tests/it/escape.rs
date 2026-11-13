//! No command prints a complaint with a control character in it: a path the caller
//! typed, and a name found in a folder, reach the terminal escaped.

use std::path::Path;
use std::process::Command;

use whizbang_testkit::scratch::Scratch;

const ID: &str = "sha256:1e7a8cc2abf001a24383094ae2ec93aee7cb27c87850972f6ca42c731ee2295f";

/// Runs `whizbang`, which must fail with exit 1, and gives what it complained of.
fn complaint(args: &[&str], store: &Path) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_whizbang"))
        .args(args)
        .arg("--store")
        .arg(store)
        .output()
        .expect("the binary runs");
    let err = String::from_utf8(out.stderr).expect("errors are UTF-8");
    assert_eq!(out.status.code(), Some(1), "{args:?}: {err:?}");
    assert!(out.stdout.is_empty());
    err
}

/// The two stores no command opens: a path with an escape sequence in it that leads
/// nowhere, and a folder that is no store and holds a file with one in its name.
fn each_bad_store(args: &[&str]) {
    let dir = Scratch::new("escape");
    let nowhere = dir.path().join("no\u{1b}[31mstore");
    let folder = dir.path().join("folder");
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(folder.join("a\u{1b}[2Jfile"), b"x").unwrap();
    for store in [&nowhere, &folder] {
        let err = complaint(args, store);
        assert!(err.starts_with("whizbang: "), "{err:?}");
        assert!(!err.contains('\u{1b}'), "{args:?}: {err:?}");
        assert_eq!(err.lines().count(), 1, "{err:?}");
        // The name is there, escaped, where the complaint names it.
        if err.contains("31mstore") || err.contains("2Jfile") {
            assert!(err.contains("\\u{1b}["), "{err:?}");
        }
    }
    let err = complaint(args, &nowhere);
    assert!(err.contains("no\\u{1b}[31mstore"), "{err:?}");
}

#[test]
fn rascal_grow_escapes_a_failure_of_the_store() {
    each_bad_store(&["rascal", "grow", "--mask", ID, "--px", "8"]);
}

#[test]
fn store_list_escapes_a_failure_of_the_store() {
    each_bad_store(&["store", "list"]);
}

#[test]
fn store_prune_and_delete_escape_a_failure_of_the_store() {
    each_bad_store(&["store", "prune"]);
    each_bad_store(&["store", "delete", "1-0"]);
}

#[test]
fn runs_list_and_show_escape_a_failure_of_the_store() {
    each_bad_store(&["runs", "list"]);
    each_bad_store(&["runs", "show", "1-0"]);
}

#[test]
fn store_import_escapes_the_file_it_cannot_read_and_the_store_it_makes() {
    let dir = Scratch::new("escape");
    let file = dir.path().join("a\u{1b}[2Jfile.mask");
    let err = complaint(
        &["store", "import", file.to_str().unwrap()],
        &dir.path().join("store"),
    );
    assert!(
        !err.contains('\u{1b}') && err.contains("a\\u{1b}[2Jfile.mask"),
        "{err:?}"
    );
    std::fs::write(&file, b"not an artifact").unwrap();
    let err = complaint(
        &["store", "import", file.to_str().unwrap()],
        &dir.path().join("store"),
    );
    assert!(
        !err.contains('\u{1b}') && err.contains("a\\u{1b}[2Jfile.mask"),
        "{err:?}"
    );
    // A file it can read, and the two stores it cannot open.
    std::fs::write(&file, super::rascal::fixture().to_bytes()).unwrap();
    each_bad_store(&["store", "import", file.to_str().unwrap()]);
}
