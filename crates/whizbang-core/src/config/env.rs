//! Keys from an env file (ADR 0011, "The env file").
//!
//! A config names the variable a credential is in; the value is in an env file, which
//! git ignores and nothing here writes. The file is read first and whole, as untrusted
//! input. Every value in it that could be a secret is one the config's reader is told
//! of ([`EnvFile::secrets`]), so nothing in the file is said back from a config; then
//! the variables the config names are resolved from the same file
//! ([`EnvFile::credentials`]), each handed to the backend whose table names it and to
//! no other. No value is in an error, a `{:?}` or a path of this module: a refusal has
//! the number of a line, or the name of a variable the config named, and nothing else
//! from the file.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read as _;
use std::path::Path;

use super::{CAPABILITIES, Config, VARIABLE_LIMIT};
use crate::contract::{Capability, SECRET_MIN_BYTES, Secrets};

/// The longest env file, in bytes. A longer one is refused from its length, before it
/// is read.
pub const ENV_BYTES_LIMIT: u64 = 64 * 1024;

/// The most variables an env file may give.
pub const ENV_VARIABLES_LIMIT: usize = 256;

/// The longest value of one variable, in bytes.
pub const ENV_VALUE_LIMIT: usize = 4096;

/// Why an env file is not read, or why a credential is not made from it. Nothing of a
/// line is ever in one: a line may be a key pasted where it does not belong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvRefused {
    /// The file could not be opened or read. The kind of failure, and no path.
    Unreadable(std::io::ErrorKind),
    /// The file is longer than this many bytes.
    TooLarge { limit: u64 },
    /// The bytes are not UTF-8, or hold a control character, first at this line.
    NotText { line: usize },
    /// This line is not `NAME=value`.
    NotAssignment { line: usize },
    /// This line gives a name the file has already given.
    Twice { line: usize },
    /// The value on this line cannot be taken for one thing: it holds a space outside
    /// quotes, a quote that is not closed or not at its ends, a backslash inside double
    /// quotes, or more than [`ENV_VALUE_LIMIT`] bytes.
    Value { line: usize },
    /// The file gives more than this many variables.
    TooMany { limit: usize },
    /// The value of a variable the config names is shorter than a secret may be.
    TooShort { variable: String },
    /// A variable the config names has, as its name, a value of the file. The name is
    /// not said.
    NameIsAValue,
}

impl std::fmt::Display for EnvRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EnvRefused::Unreadable(kind) => {
                let why = match kind {
                    std::io::ErrorKind::NotFound => "it is not there",
                    std::io::ErrorKind::PermissionDenied => "permission is denied",
                    std::io::ErrorKind::IsADirectory => "it is a folder",
                    _ => "the system could not read it",
                };
                write!(f, "the env file cannot be read: {why}")
            }
            EnvRefused::TooLarge { limit } => {
                write!(f, "the env file is longer than {limit} bytes")
            }
            EnvRefused::NotText { line } => write!(
                f,
                "the env file is not text at line {line}: it is UTF-8 with no control character"
            ),
            EnvRefused::NotAssignment { line } => write!(
                f,
                "line {line} of the env file is not an assignment: NAME=value, with a name \
                 of capital letters, digits and _; the line is not shown"
            ),
            EnvRefused::Twice { line } => write!(
                f,
                "line {line} of the env file gives a name the file has already given; the \
                 line is not shown"
            ),
            EnvRefused::Value { line } => write!(
                f,
                "the value on line {line} of the env file is not one value: put a value \
                 with a space in single quotes, close every quote, and keep it under \
                 {ENV_VALUE_LIMIT} bytes; the line is not shown"
            ),
            EnvRefused::TooMany { limit } => {
                write!(f, "the env file gives more than {limit} variables")
            }
            EnvRefused::TooShort { variable } => write!(
                f,
                "the env file's value for {variable} is shorter than {SECRET_MIN_BYTES} \
                 bytes, which is too short to be a credential or to be kept out of what is \
                 written; the value is not shown"
            ),
            EnvRefused::NameIsAValue => f.write_str(
                "a variable the config names has a value of the env file as its name; \
                 neither is shown",
            ),
        }
    }
}

impl std::error::Error for EnvRefused {}

/// What one env file gives: the value of each variable it sets. It shows how many it
/// holds and nothing else.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct EnvFile {
    /// By the variable's name. A variable set to nothing is not here.
    values: BTreeMap<String, String>,
}

impl std::fmt::Debug for EnvFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EnvFile({})", self.values.len())
    }
}

/// Whether this is a variable's name: capital letters, digits and `_`, not a digit
/// first, [`VARIABLE_LIMIT`] characters at most. The rule the config holds a name to.
fn is_variable(name: &str) -> bool {
    let first = name
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_uppercase() || byte == b'_');
    let rest = name
        .bytes()
        .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_');
    first && rest && name.len() <= VARIABLE_LIMIT
}

/// The value written after a `=`, without the quotes around it, or nothing when it
/// cannot be taken for one value.
fn value_of(written: &str) -> Option<&str> {
    let written = written.trim_matches(|c: char| c.is_ascii_whitespace());
    let value = match written.as_bytes() {
        [b'\'', .., b'\''] => {
            let inner = &written[1..written.len() - 1];
            (!inner.contains('\'')).then_some(inner)?
        }
        [b'"', .., b'"'] => {
            let inner = &written[1..written.len() - 1];
            (!inner.contains(['"', '\\'])).then_some(inner)?
        }
        _ => {
            let plain =
                !written.contains(|c: char| c.is_ascii_whitespace() || c == '\'' || c == '"');
            plain.then_some(written)?
        }
    };
    (value.len() <= ENV_VALUE_LIMIT).then_some(value)
}

impl EnvFile {
    /// The env file of no file: no variable is set.
    pub fn empty() -> EnvFile {
        EnvFile::default()
    }

    /// Reads the env file at this path. Its length is asked for before it is opened,
    /// and a file over [`ENV_BYTES_LIMIT`] is refused from it; no more than the limit
    /// and one byte is read of a file that is longer than it said.
    pub fn read(path: &Path) -> Result<EnvFile, EnvRefused> {
        let unreadable = |error: std::io::Error| EnvRefused::Unreadable(error.kind());
        let too_large = EnvRefused::TooLarge {
            limit: ENV_BYTES_LIMIT,
        };
        if std::fs::metadata(path).map_err(unreadable)?.len() > ENV_BYTES_LIMIT {
            return Err(too_large);
        }
        let file = std::fs::File::open(path).map_err(unreadable)?;
        let mut bytes = Vec::new();
        file.take(ENV_BYTES_LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(unreadable)?;
        EnvFile::parse(&bytes)
    }

    /// Reads these bytes as an env file. A line is empty, a comment that starts with
    /// `#`, or `NAME=value`, with `export` and a space before it if it likes. The value
    /// loses the white space at its ends and one pair of single or double quotes around
    /// it. Nothing is expanded and no escape is read.
    pub fn parse(bytes: &[u8]) -> Result<EnvFile, EnvRefused> {
        if bytes.len() as u64 > ENV_BYTES_LIMIT {
            return Err(EnvRefused::TooLarge {
                limit: ENV_BYTES_LIMIT,
            });
        }
        let line_of = |at: usize| 1 + bytes[..at].iter().filter(|byte| **byte == b'\n').count();
        let text = std::str::from_utf8(bytes).map_err(|error| EnvRefused::NotText {
            line: line_of(error.valid_up_to()),
        })?;
        let control = |byte: &u8| (*byte < 0x20 && !b"\t\n\r".contains(byte)) || *byte == 0x7f;
        if let Some(at) = bytes.iter().position(control) {
            return Err(EnvRefused::NotText { line: line_of(at) });
        }
        let mut names: BTreeSet<&str> = BTreeSet::new();
        let mut values = BTreeMap::new();
        for (index, line) in text.split('\n').enumerate() {
            let line_number = index + 1;
            let line = line.trim_matches(|c: char| c.is_ascii_whitespace());
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let line = match line.strip_prefix("export") {
                Some(rest) if rest.starts_with([' ', '\t']) => rest.trim_start_matches([' ', '\t']),
                _ => line,
            };
            let Some((name, written)) = line.split_once('=') else {
                return Err(EnvRefused::NotAssignment { line: line_number });
            };
            if !is_variable(name) {
                return Err(EnvRefused::NotAssignment { line: line_number });
            }
            let Some(value) = value_of(written) else {
                return Err(EnvRefused::Value { line: line_number });
            };
            if !names.insert(name) {
                return Err(EnvRefused::Twice { line: line_number });
            }
            if names.len() > ENV_VARIABLES_LIMIT {
                return Err(EnvRefused::TooMany {
                    limit: ENV_VARIABLES_LIMIT,
                });
            }
            if !value.is_empty() {
                values.insert(name.to_owned(), value.to_owned());
            }
        }
        Ok(EnvFile { values })
    }

    /// Every value of the file that could be a secret, which is every one of at least
    /// [`SECRET_MIN_BYTES`] bytes, whether or not a config names its variable. A config
    /// is read with these, so that nothing in the env file is said back from a config.
    pub fn secrets(&self) -> Secrets {
        let long = self
            .values
            .values()
            .filter(|value| value.len() >= SECRET_MIN_BYTES)
            .cloned();
        // No value here is shorter than a secret may be, which is all `new` refuses.
        Secrets::new(long).unwrap_or_default()
    }

    /// The credentials of a config: for each backend, the value of each variable its
    /// table names, where the file sets it. A variable the file does not set is listed
    /// as missing and is no refusal. One whose value is shorter than a secret may be
    /// is refused by its name.
    pub fn credentials(&self, config: &Config) -> Result<Credentials, EnvRefused> {
        // A name is said in a refusal and listed as missing. One that holds a value of
        // this file that could be a secret is a key written where its variable's name
        // belongs.
        let named = config.variables();
        if named
            .iter()
            .any(|name| self.secrets().redact(name) != *name)
        {
            return Err(EnvRefused::NameIsAValue);
        }
        let mut credentials = Credentials::none();
        let mut values = BTreeSet::new();
        for capability in CAPABILITIES {
            for (name, backend) in config.backends(capability) {
                let mut handed = Handed::default();
                for (key, variable) in backend.credentials() {
                    let Some(value) = self.values.get(variable) else {
                        credentials.missing.insert(variable.to_owned());
                        continue;
                    };
                    if value.len() < SECRET_MIN_BYTES {
                        return Err(EnvRefused::TooShort {
                            variable: variable.to_owned(),
                        });
                    }
                    values.insert(value.clone());
                    let credential = Some(Credential(value.clone()));
                    match key {
                        "key_env" => handed.key = credential,
                        _ => handed.token = credential,
                    }
                }
                credentials
                    .handed
                    .insert((capability.name(), name.to_owned()), handed);
            }
        }
        // Each is at least as long as a secret must be.
        credentials.secrets = Secrets::new(values).unwrap_or_default();
        Ok(credentials)
    }
}

/// The credentials of one config, each under the backend whose table names it. There
/// is no question here by a variable's name, and none that answers every credential.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Credentials {
    /// By the capability's name and the backend's.
    handed: BTreeMap<(&'static str, String), Handed>,
    missing: BTreeSet<String>,
    secrets: Secrets,
}

impl Credentials {
    /// No credential: what no config and no env file give.
    pub fn none() -> Credentials {
        Credentials::default()
    }

    /// What this backend is handed: the credentials its own table names, and no
    /// other's. A backend the config does not have is handed nothing.
    pub fn of(&self, capability: Capability, backend: &str) -> Handed {
        self.handed
            .get(&(capability.name(), backend.to_owned()))
            .cloned()
            .unwrap_or_default()
    }

    /// The secrets the executor takes out of what it writes: every credential the
    /// config names that has a value.
    pub fn secrets(&self) -> &Secrets {
        &self.secrets
    }

    /// The name of each variable the config names that the file does not set, in the
    /// order of their bytes.
    pub fn missing(&self) -> impl Iterator<Item = &str> {
        self.missing.iter().map(String::as_str)
    }
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field("backends", &self.handed.len())
            .field("missing", &self.missing.len())
            .field("secrets", &self.secrets)
            .finish()
    }
}

/// What one backend is handed.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Handed {
    key: Option<Credential>,
    token: Option<Credential>,
}

impl Handed {
    /// The credential the backend's `key_env` names, when the file sets it.
    pub fn key(&self) -> Option<&Credential> {
        self.key.as_ref()
    }

    /// The credential the backend's `token_env` names, when the file sets it.
    pub fn token(&self) -> Option<&Credential> {
        self.token.as_ref()
    }
}

impl std::fmt::Debug for Handed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Handed")
            .field("key", &self.key.is_some())
            .field("token", &self.token.is_some())
            .finish()
    }
}

/// One credential. It is never printed: its `{:?}` shows that it is one, and it has no
/// other way to a text than [`Credential::expose`].
#[derive(Clone, PartialEq, Eq)]
pub struct Credential(String);

impl Credential {
    /// The secret itself, for the one place a backend sends it.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Credential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Credential(..)")
    }
}
