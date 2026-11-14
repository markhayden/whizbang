//! Why a config file is not read (ADR 0011, "What a refusal says").
//!
//! A refusal names a key and never shows a value, and it holds no path: the caller
//! knows which file it asked for. What it shows of a key is escaped and cut, and is
//! looked at for a secret, whole, before either.

/// How many characters a refusal shows of a key that came from the file, before the
/// three dots that mark a cut.
pub const KEY_SHOWN_LIMIT: usize = 64;

/// Why a config file is not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigRefused {
    /// The file could not be opened or read. The kind of failure, and no path.
    Unreadable(std::io::ErrorKind),
    /// What the path names is not a file: a folder, a pipe, a device.
    NotAFile,
    /// The file is longer than this many bytes.
    TooLarge { limit: u64 },
    /// The bytes are not UTF-8, or hold a control character, first at this line.
    NotText { line: usize },
    /// The file holds a secret the reader was told of. Nothing of the file is shown.
    HoldsSecret,
    /// The text is not TOML, first at this line. None of its words is shown.
    NotToml { line: usize },
    /// A key is given twice, the second time at this line. The key is its last part,
    /// without the table it is in.
    Twice { key: String, line: usize },
    /// A key is not known, is missing, or has a value that does not belong there. The
    /// key is given from the top of the file, with dots, and the reason holds fixed
    /// words and numbers only, never the value.
    Key { key: String, why: String },
}

impl std::fmt::Display for ConfigRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigRefused::Unreadable(kind) => {
                let why = match kind {
                    std::io::ErrorKind::NotFound => "it is not there",
                    std::io::ErrorKind::PermissionDenied => "permission is denied",
                    _ => "the system could not read it",
                };
                write!(f, "the config file cannot be read: {why}")
            }
            ConfigRefused::NotAFile => {
                f.write_str("the config file cannot be read: it is not a file")
            }
            ConfigRefused::TooLarge { limit } => {
                write!(f, "the config file is longer than {limit} bytes")
            }
            ConfigRefused::NotText { line } => write!(
                f,
                "the config file is not text at line {line}: it is UTF-8 with no control \
                 character"
            ),
            ConfigRefused::HoldsSecret => f.write_str(
                "the config file holds a secret: a credential is named by its variable and \
                 is never written in the file; nothing of the file is shown",
            ),
            ConfigRefused::NotToml { line } => {
                write!(f, "the config file is not TOML at line {line}")
            }
            ConfigRefused::Twice { key, line } => write!(
                f,
                "the config gives the key `{key}` twice, the second time at line {line}"
            ),
            ConfigRefused::Key { key, why } => write!(f, "the config's key `{key}` {why}"),
        }
    }
}

impl std::error::Error for ConfigRefused {}

/// A key that came from the file, as a refusal shows it: each character escaped, so
/// that what is shown is one line of ASCII, and the first [`KEY_SHOWN_LIMIT`]
/// characters of that, with three dots when there were more. The caller has looked at
/// the whole key for a secret.
pub(super) fn shown(key: &str) -> String {
    let mut escaped = key.chars().flat_map(char::escape_default);
    let mut shown: String = escaped.by_ref().take(KEY_SHOWN_LIMIT).collect();
    if escaped.next().is_some() {
        shown.push_str("...");
    }
    shown
}
