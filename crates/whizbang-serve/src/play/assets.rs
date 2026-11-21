//! The interface's files, embedded in the binary from a fixed list (ADR 0041; the plan's
//! "How it ships"). Until the built interface (`ui/dist/`, task U1) lands, the list holds
//! one placeholder page; U1 replaces it with `include_bytes!` of each built file, and a
//! test then holds the list to the folder.

/// The placeholder page: it says what it is, loads nothing, and runs nothing.
const PLACEHOLDER: &str = "<!doctype html>\n<html lang=\"en\" data-theme=\"dark\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>whizbang playground</title>\n</head>\n<body>\n<h1>whizbang</h1>\n<p>The playground's host is running. Its interface arrives in a later build.</p>\n</body>\n</html>\n";

/// Each file: its path under the host's root, its type, and its bytes.
const FILES: &[(&str, &str, &[u8])] = &[(
    "index.html",
    "text/html; charset=utf-8",
    PLACEHOLDER.as_bytes(),
)];

/// The file at this path (without its leading `/`), when the list holds one: its type
/// and its bytes. Nothing outside the list is ever read.
pub fn find(path: &str) -> Option<(&'static str, &'static [u8])> {
    FILES
        .iter()
        .find(|(named, _, _)| *named == path)
        .map(|(_, content_type, bytes)| (*content_type, *bytes))
}

/// The paths of every file the list holds.
pub fn paths() -> impl Iterator<Item = &'static str> {
    FILES.iter().map(|(path, _, _)| *path)
}
