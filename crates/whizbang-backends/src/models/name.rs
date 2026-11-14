//! The texts that come from outside the build and become names on a disk or words in a
//! record (ADR 0017, "Names"): a model's name, a file's name, a revision and a
//! licence.
//!
//! A model's name is the name of a folder and a file's is the name of a file, so each
//! is one plain name: it starts with a letter or a digit and holds no separator, so it
//! is never `.` or `..`, never hidden, never a path and never an option to a command.
//! Each constructor looks at the whole text for a secret first.

use whizbang_core::contract::Secrets;

use super::{Refused, Rule};

/// The most characters of a model's name, and of an adapter's.
pub const MODEL_NAME_LIMIT: usize = 64;

/// The most characters of a file's name.
pub const FILE_NAME_LIMIT: usize = 128;

/// The most characters of a revision.
pub const REVISION_LIMIT: usize = 64;

/// The most characters of a licence's name.
pub const LICENCE_LIMIT: usize = 200;

/// The most bytes of one file: a tebibyte.
pub const FILE_MAX_BYTES: u64 = 1 << 40;

/// How many characters of a name are shown, before the three dots that mark a cut.
pub const SHOWN_LIMIT: usize = 64;

/// Whether a text holds a secret the reader was told of, in any form redaction finds.
pub(super) fn holds_secret(text: &str, secrets: &Secrets) -> bool {
    secrets.redact(text) != text
}

/// A text of 1 to `limit` ASCII characters that starts with a letter or a digit and
/// goes on with letters, digits and these marks; in lower case only, when `lower`.
fn plain(text: &str, limit: usize, marks: &[u8], lower: bool) -> bool {
    let letter =
        |b: u8| b.is_ascii_digit() || b.is_ascii_lowercase() || (!lower && b.is_ascii_uppercase());
    text.len() <= limit
        && text.bytes().next().is_some_and(letter)
        && text.bytes().all(|b| letter(b) || marks.contains(&b))
}

/// A name as it is shown: its first [`SHOWN_LIMIT`] characters, with three dots when
/// there were more. Every character of a name is printable ASCII, so nothing of it
/// needs an escape and a cut cannot fall inside a character.
fn shown(name: &str) -> String {
    match name.get(..SHOWN_LIMIT) {
        Some(start) if name.len() > SHOWN_LIMIT => format!("{start}..."),
        _ => name.to_owned(),
    }
}

macro_rules! name {
    ($(#[$doc:meta])* $name:ident, $rule:expr, $keeps:expr) => {
        $(#[$doc])*
        #[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            /// The text as this, when it keeps the rule. A text that holds a secret
            /// the constructor was told of is refused first, whole, whatever else is
            /// wrong with it. The refusal shows nothing of the text.
            pub fn new(text: &str, secrets: &Secrets) -> Result<$name, Refused> {
                if holds_secret(text, secrets) {
                    return Err(Refused::HoldsSecret);
                }
                let keeps: fn(&str) -> bool = $keeps;
                if !keeps(text) {
                    return Err(Refused::rule($rule));
                }
                Ok($name(text.to_owned()))
            }

            /// The text, whole.
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// The text as a refusal or a status shows it: its first
            /// [`SHOWN_LIMIT`] characters, and three dots when there are more.
            pub fn shown(&self) -> String {
                shown(&self.0)
            }
        }

        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}({})", stringify!($name), self.shown())
            }
        }
    };
}

name!(
    /// The name of a model in a catalogue, and of an adapter: a lower-case letter or a
    /// digit, then lower-case letters, digits, `_`, `.` and `-`, at most
    /// [`MODEL_NAME_LIMIT`] characters. It is the name of the model's folder, and it is
    /// in lower case only so that two models are two folders on a disk that does not
    /// tell the cases of letters apart.
    ModelName,
    Rule::ModelName,
    |text| plain(text, MODEL_NAME_LIMIT, b"_.-", true)
);

name!(
    /// The name of one file of a model: a letter or a digit, then letters, digits,
    /// `_`, `.`, `-` and `+`, at most [`FILE_NAME_LIMIT`] characters. It is one plain
    /// name: no separator, not `.` or `..`, not hidden.
    FileName,
    Rule::FileName,
    |text| plain(text, FILE_NAME_LIMIT, b"_.-+", false)
);

name!(
    /// A revision, of a catalogue's entry or of an adapter: a letter or a digit, then
    /// letters, digits, `_`, `.` and `-`, at most [`REVISION_LIMIT`] characters. Two
    /// revisions are the same when their texts are; nothing orders them.
    Revision,
    Rule::Revision,
    |text| plain(text, REVISION_LIMIT, b"_.-", false)
);

name!(
    /// The licence a model's files are under, as their maker states it: 1 to
    /// [`LICENCE_LIMIT`] printable ASCII characters and spaces, with no space at
    /// either end.
    Licence,
    Rule::Licence,
    |text| {
        (1..=LICENCE_LIMIT).contains(&text.len())
            && text.bytes().all(|b| b.is_ascii_graphic() || b == b' ')
            && !text.starts_with(' ')
            && !text.ends_with(' ')
    }
);

/// The rules a list of files keeps, in a catalogue's entry and in an installation
/// record alike: one file at least, `most` at most, and no two of one name. Two names
/// that differ only in the case of their letters are one name, as they are one file on
/// a disk that does not tell the cases apart.
pub(super) fn files_named<'a>(
    names: impl ExactSizeIterator<Item = &'a FileName>,
    most: usize,
) -> Result<(), Refused> {
    if names.len() == 0 {
        return Err(Refused::at("files", Rule::NoFiles));
    }
    if names.len() > most {
        return Err(Refused::at("files", Rule::Count { most }));
    }
    let mut seen = std::collections::BTreeSet::new();
    for name in names {
        if !seen.insert(name.as_str().to_ascii_lowercase()) {
            return Err(Refused::at("files", Rule::NamedTwice));
        }
    }
    Ok(())
}
