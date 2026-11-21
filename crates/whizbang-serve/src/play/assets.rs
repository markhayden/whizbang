//! The playground's built interface, `ui/dist/`, embedded in the binary (plan m5, "How it
//! ships"; ADR 0043). The build writes fixed names with no hash in them, so the list is
//! fixed: `scripts/ui.sh check` (part of `just check`) fails on a file of `ui/dist/` that
//! this list does not hold, or the reverse, and on a `ui/dist/` that is not what the
//! sources build. So `cargo build` needs no Node, and the bytes that ship are the bytes
//! that were reviewed and scanned.

/// One file: its path under the host's root, its type, and its bytes.
type File = (&'static str, &'static str, &'static [u8]);

macro_rules! asset {
    ($path:literal, $type:literal) => {
        (
            $path,
            $type,
            include_bytes!(concat!("../../../../ui/dist/", $path)),
        )
    };
}

/// Every file of `ui/dist/`, sorted by path.
const FILES: &[File] = &[
    asset!("app.css", "text/css; charset=utf-8"),
    asset!("app.js", "text/javascript; charset=utf-8"),
    asset!("fonts/bricolage-grotesque-800.woff2", "font/woff2"),
    asset!("fonts/geist-400.woff2", "font/woff2"),
    asset!("fonts/geist-500.woff2", "font/woff2"),
    asset!("fonts/geist-600.woff2", "font/woff2"),
    asset!("fonts/geist-700.woff2", "font/woff2"),
    asset!("fonts/geist-mono-400.woff2", "font/woff2"),
    asset!("fonts/geist-mono-600.woff2", "font/woff2"),
    asset!("index.html", "text/html; charset=utf-8"),
];

/// The page itself, which the host serves at `/`.
pub const INDEX: &str = "index.html";

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
