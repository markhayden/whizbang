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

#[test]
fn printable_escapes_every_control_character_and_leaves_nothing_a_terminal_obeys() {
    // Every control character there is: the C0 set with the carriage return, the line
    // break and the tab, DEL, and the C1 set, which some terminals read as the start
    // of a sequence. And what turns text around or breaks a line without being a
    // control: the line and paragraph separators and a right-to-left override.
    let mut marks: Vec<char> = (0u32..0x20).chain(0x7f..0xa0).map(control).collect();
    marks.extend(['\u{2028}', '\u{2029}', '\u{202e}', '\u{200b}', '\u{feff}']);
    for mark in marks {
        let said = whizbang::door::printable(&format!("a{mark}b"));
        assert!(said.is_ascii(), "{:?} is printed as {said:?}", mark as u32);
        assert!(!said.chars().any(char::is_control), "{:?}", mark as u32);
        assert!(said.starts_with("a\\") && said.ends_with('b'), "{said:?}");
    }
    assert_eq!(whizbang::door::printable("a\rb\tc\nd"), "a\\rb\\tc\\nd");
    assert_eq!(
        whizbang::door::printable("\u{7}\u{8}\u{9b}"),
        "\\u{7}\\u{8}\\u{9b}"
    );
    // Quotes and the backslash are left, so escaping twice is escaping once.
    let once = whizbang::door::printable("it's \"so\"\r");
    assert_eq!(once, "it's \"so\"\\r");
    assert_eq!(whizbang::door::printable(&once), once);
    // Letters of any script are printed as they are.
    assert_eq!(
        whizbang::door::printable("caf\u{e9} \u{65e5}"),
        "caf\u{e9} \u{65e5}"
    );
}

fn control(code: u32) -> char {
    char::from_u32(code).expect("a character")
}

#[test]
fn store_import_escapes_the_path_of_the_store_it_made() {
    // The line that says a store was made is on the standard output, which nothing
    // else escapes: the path is printed escaped, and the store is at the path given.
    let dir = Scratch::new("escape");
    let file = dir.path().join("seam.mask");
    std::fs::write(&file, super::rascal::fixture().to_bytes()).unwrap();
    let store = dir.path().join("a\r\u{1b}[2Jstore");
    let out = Command::new(env!("CARGO_BIN_EXE_whizbang"))
        .args(["store", "import", file.to_str().unwrap(), "--create"])
        .arg("--store")
        .arg(&store)
        .output()
        .expect("the binary runs");
    let printed = String::from_utf8(out.stdout).expect("output is UTF-8");
    assert_eq!(out.status.code(), Some(0), "{printed}");
    assert!(store.join("CURRENT").exists());
    let first = printed.lines().next().unwrap();
    assert!(first.starts_with("created a store at "), "{printed:?}");
    assert!(first.ends_with("a\\r\\u{1b}[2Jstore"), "{printed:?}");
    assert!(
        !printed.chars().any(|c| c.is_control() && c != '\n'),
        "{printed:?}"
    );
    assert_eq!(printed.lines().count(), 2, "{printed:?}");
}

/// Runs `whizbang` with these arguments and gives its exit code and its two outputs.
fn ran(args: &[&str]) -> (Option<i32>, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_whizbang"))
        .args(args)
        .output()
        .expect("the binary runs");
    let text = |bytes: Vec<u8>| String::from_utf8(bytes).expect("output is UTF-8");
    (out.status.code(), text(out.stdout), text(out.stderr))
}

#[test]
fn a_usage_error_shows_what_was_typed_with_no_control_character() {
    // The parser of the command line answers a mistake itself, and quotes what was
    // typed: a value that is no number, a command that is none, an argument too many,
    // a flag that is none. What it says is printed with every control character but
    // its own line breaks escaped; the exit code is 2, as it was.
    let typed = "1\r\u{1b}[31mX\u{7}";
    let cases: [&[&str]; 4] = [
        &[
            "rascal", "grow", "--mask", ID, "--px", typed, "--store", "s",
        ],
        &[typed],
        &["runs", "show", "a", typed, "--store", "s"],
        &["store", "list", "--store", "s", &format!("--{typed}")],
    ];
    for args in cases {
        let (code, out, err) = ran(args);
        assert_eq!(code, Some(2), "{args:?}: {err:?}");
        assert!(out.is_empty(), "{out:?}");
        assert!(
            !err.chars().any(|c| c.is_control() && c != '\n'),
            "{args:?}: {err:?}"
        );
        // The carriage return is there, escaped. (The parser's own printing drops an
        // escape sequence it is given when it does not write to a terminal; nothing
        // here relies on that.)
        assert!(err.contains("1\\r"), "{args:?}: {err:?}");
        assert!(err.starts_with("error: "), "{err:?}");
        assert!(err.contains("--help"), "{err:?}");
        assert!(err.ends_with('\n'), "{err:?}");
        // The parser's own line breaks are line breaks still: the error, an empty
        // line, and where to read more, and no line break written out as text.
        let lines: Vec<&str> = err.lines().collect();
        assert!(lines.len() >= 3 && lines.contains(&""), "{err:?}");
        assert!(lines.last().unwrap().contains("--help"), "{err:?}");
        assert!(!err.contains("\\n"), "{err:?}");
    }
    // Help and the version are the parser's too, and are printed as they were.
    let (code, out, err) = ran(&["--help"]);
    assert_eq!((code, err.as_str()), (Some(0), ""));
    assert!(out.starts_with("One toolkit for working on images with models\n"));
    let (code, out, _) = ran(&["runs", "show", "--help"]);
    assert_eq!(code, Some(0));
    assert!(out.contains("\nUsage: whizbang runs show"), "{out}");
}
