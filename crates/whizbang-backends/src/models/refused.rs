//! Why something of the models directory is not read, or not made (ADR 0017, "What a
//! refusal says").
//!
//! A refusal is fixed words, a place in a record and numbers. It quotes nothing of
//! what was refused, whether or not that held a secret, and it holds no path: the
//! caller knows what it asked for.

use whizbang_core::egress::NotAllowed;

use super::{FILE_MAX_BYTES, FILE_NAME_LIMIT, LICENCE_LIMIT, MODEL_NAME_LIMIT, REVISION_LIMIT};

/// Which of the two records a refusal is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum What {
    /// The catalogue, `catalogue/1`.
    Catalogue,
    /// An installation record, `installation/1`.
    Installation,
}

impl std::fmt::Display for What {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            What::Catalogue => "the catalogue",
            What::Installation => "the installation record",
        })
    }
}

/// The rule a value does not keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Rule {
    /// An object holds a field that is not one of its fields. The place is the object.
    Unknown,
    /// A field is not there.
    Missing,
    /// A value is of another kind than its field's: a number where a text belongs, a
    /// number that is not a whole number from 0 to what 64 bits hold, a list where an
    /// object belongs.
    Kind,
    /// The format is not the one this reader reads.
    Format,
    /// Not a model's name, or an adapter's.
    ModelName,
    /// Not one plain file name.
    FileName,
    /// Not a revision.
    Revision,
    /// Not a licence's name.
    Licence,
    /// Not a feature's name.
    Feature,
    /// Not `sha256:` and 64 lower-case hex digits.
    Hash,
    /// A file's size is zero, or over the most a file may be.
    Size,
    /// A list holds more than this many.
    Count { most: usize },
    /// A model has no file.
    NoFiles,
    /// A list names something twice. Two files are one when their names differ only in
    /// the case of their letters.
    NamedTwice,
    /// Not an origin (ADR 0012), for this reason.
    Origin(NotAllowed),
    /// An origin written with a path after it.
    OriginAlone,
    /// Not an address (ADR 0012), for this reason.
    Address(NotAllowed),
    /// A file's address is at an origin its model does not name.
    Elsewhere,
}

impl std::fmt::Display for Rule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Rule::Unknown => f.write_str("holds a field that is not one of its fields"),
            Rule::Missing => f.write_str("is missing"),
            Rule::Kind => f.write_str("is of another kind than its field"),
            Rule::Format => f.write_str("is not the format this reader reads"),
            Rule::ModelName => write!(
                f,
                "is not a model's name: a lower-case letter or a digit, then lower-case \
                 letters, digits, underscores, dots and hyphens, {MODEL_NAME_LIMIT} \
                 characters at most"
            ),
            Rule::FileName => write!(
                f,
                "is not one plain file name: a letter or a digit, then letters, digits, \
                 underscores, dots, hyphens and plus signs, {FILE_NAME_LIMIT} characters \
                 at most"
            ),
            Rule::Revision => write!(
                f,
                "is not a revision: a letter or a digit, then letters, digits, underscores, \
                 dots and hyphens, {REVISION_LIMIT} characters at most"
            ),
            Rule::Licence => write!(
                f,
                "is not a licence's name: 1 to {LICENCE_LIMIT} printable ASCII characters \
                 and spaces, with no space at either end"
            ),
            Rule::Feature => f.write_str(
                "is not a feature's name: a lower-case letter, then lower-case letters, \
                 digits, underscores and hyphens, 64 characters at most",
            ),
            Rule::Hash => f.write_str("is not sha256, a colon and 64 lower-case hex digits"),
            Rule::Size => write!(f, "is not a size from 1 byte to {FILE_MAX_BYTES} bytes"),
            Rule::Count { most } => write!(f, "holds more than {most}"),
            Rule::NoFiles => f.write_str("holds no file, and a model has one at least"),
            Rule::NamedTwice => f.write_str(
                "names one thing twice (two files are one when only the case of their \
                 letters differs)",
            ),
            Rule::Origin(why) => write!(f, "is not an origin: {why}"),
            Rule::OriginAlone => {
                f.write_str("is an origin with a path after it, and an origin has none")
            }
            Rule::Address(why) => write!(f, "is not an address: {why}"),
            Rule::Elsewhere => f.write_str("is at an origin its model does not name"),
        }
    }
}

/// Why a catalogue or an installation record is not read, or why a value of one is not
/// made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// The source could not be read. The kind of failure, and no path.
    Unreadable {
        what: What,
        kind: std::io::ErrorKind,
    },
    /// The bytes are more than this many. Nothing of them was looked at.
    TooLarge { what: What, limit: u64 },
    /// A text holds a secret the reader was told of, or what would be written of a
    /// value does. Nothing of it is shown.
    HoldsSecret,
    /// The bytes are not JSON, first at this line.
    NotJson { what: What, line: usize },
    /// An object gives one field twice, the second time at this line.
    Twice { what: What, line: usize },
    /// The bytes say what a record says, and are not the one form whizbang writes.
    OtherForm { what: What },
    /// A value does not keep its rule. `at` is its place in the record, as field names
    /// and positions (`models[0].files[1].size`), and is empty when the value was given
    /// to a constructor by itself.
    Value { at: String, rule: Rule },
}

impl Refused {
    /// A value by itself that does not keep this rule.
    pub(super) fn rule(rule: Rule) -> Refused {
        Refused::Value {
            at: String::new(),
            rule,
        }
    }

    /// The same refusal, of a value at this place.
    pub(super) fn at(at: &str, rule: Rule) -> Refused {
        Refused::Value {
            at: at.to_owned(),
            rule,
        }
    }

    /// The same refusal, with its place under this one: `files[0]` under `models[2]`
    /// is `models[2].files[0]`.
    pub(super) fn under(self, outer: &str) -> Refused {
        match self {
            Refused::Value { at, rule } if at.is_empty() => Refused::at(outer, rule),
            Refused::Value { at, rule } => Refused::at(&format!("{outer}.{at}"), rule),
            other => other,
        }
    }
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refused::Unreadable { what, kind } => {
                let why = match kind {
                    std::io::ErrorKind::NotFound => "it is not there",
                    std::io::ErrorKind::PermissionDenied => "permission is denied",
                    _ => "the system could not read it",
                };
                write!(f, "{what} cannot be read: {why}")
            }
            Refused::TooLarge { what, limit } => {
                write!(f, "{what} is longer than {limit} bytes")
            }
            Refused::HoldsSecret => f.write_str("the text holds a secret, and is not shown"),
            Refused::NotJson { what, line } => write!(f, "{what} is not JSON at line {line}"),
            Refused::Twice { what, line } => {
                write!(
                    f,
                    "{what} gives one field twice, the second time at line {line}"
                )
            }
            Refused::OtherForm { what } => {
                write!(f, "{what} is not in the one form it is written in")
            }
            Refused::Value { at, rule } if at.is_empty() => write!(f, "the value {rule}"),
            Refused::Value { at, rule } => write!(f, "`{at}` {rule}"),
        }
    }
}

impl std::error::Error for Refused {}
