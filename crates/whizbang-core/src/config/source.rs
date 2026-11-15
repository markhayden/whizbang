//! The source an executor asks, over the config file and its env file (ADR 0011, "The
//! source over both files"; ADR 0015, "One snapshot of the config").
//!
//! [`Files`] names the two files and holds the kinds of a build. Each time it is
//! asked it reads both files again, whole ([`Loaded::of_files`]), and answers one
//! snapshot of what they say: the config's digest, the backends its kinds made
//! ([`Offers`]) and every credential the config names. A file that cannot be read as
//! what it is refuses that ask in fixed words, which name the file as "the config" or
//! "the env file" and quote nothing of either; what the readers themselves say of it,
//! the key or the line, is [`Files::load`]'s to give, to whoever loads a config
//! directly.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use crate::contract::{ConfigSource, SECRETS_KEPT_LIMIT, Secrets, Snapshot, SourceRefused};

use super::{ConfigRefused, EnvRefused, Kinds, LoadRefused, Loaded, Offers};

/// What is said when a reading would bring more keys than a source keeps.
const KEYS_OVER: &str = "the env file has given more keys, over every reading, than a source of the config \
     keeps, and none is let go";

/// What is said when what a config offers makes no roster. A config that was read
/// gives no such thing; it is passed on and not dropped all the same.
const NO_ROSTER: &str = "the backends of the config do not make a roster";

/// The config file and its env file, as an executor's source of the config.
///
/// It keeps no reading: each ask reads both files again. What it keeps is every
/// credential a reading of its handed out, for as long as it lives, as an executor
/// keeps them (ADR 0015): a config is read with those too, so one that holds a key
/// that was since rotated out of the env file is refused whole, and a kind's
/// sentence is looked at with them. It keeps [`SECRETS_KEPT_LIMIT`] at most, and a
/// reading that would add one over that is refused: none is ever let go.
///
/// What `{:?}` prints is whether an env file is named, how many kinds it holds and
/// how many secrets it knows: no path and no key.
pub struct Files {
    config: PathBuf,
    env: Option<PathBuf>,
    kinds: Kinds,
    /// Every credential a reading handed out.
    read: Mutex<Secrets>,
}

impl Files {
    /// A source over this config file, this env file when the config names a
    /// variable, and the kinds of this build. Nothing is read until it is asked.
    pub fn new(config: &Path, env: Option<&Path>, kinds: Kinds) -> Files {
        Files {
            config: config.to_owned(),
            env: env.map(Path::to_owned),
            kinds,
            read: Mutex::new(Secrets::none()),
        }
    }

    /// The same source, told of these secrets from the start, as an executor is
    /// built with some ([`Engine::with_secrets`](crate::contract::Engine::with_secrets)):
    /// a config that holds one is refused whole. They are not counted against the
    /// bound, which is on what readings add.
    pub fn knowing(self, secrets: Secrets) -> Files {
        Files {
            read: Mutex::new(secrets),
            ..self
        }
    }

    /// Both files, read now as the source reads them for a snapshot, or the readers'
    /// own refusal: the one that names a key of the config, escaped and cut, or the
    /// line of the env file, and never a path (ADR 0011, "What a refusal says"). It
    /// is what a command that loads a config directly shows; a call is told less
    /// ([`SourceRefused`]).
    pub fn load(&self) -> Result<Loaded, LoadRefused> {
        Loaded::of_files_knowing(&self.config, self.env.as_deref(), &self.known())
    }

    /// Every credential a reading has handed out so far.
    fn known(&self) -> Secrets {
        let read = self.read.lock().unwrap_or_else(PoisonError::into_inner);
        read.clone()
    }

    /// Takes the credentials of a reading in, and answers all that are known with
    /// them. None, and nothing is taken in, when they add one and all that would
    /// then be known are more than [`SECRETS_KEPT_LIMIT`]. The look and the keeping
    /// are one step under the lock.
    fn keep(&self, more: &Secrets) -> Option<Secrets> {
        let mut read = self.read.lock().unwrap_or_else(PoisonError::into_inner);
        let joined = read.joined(more);
        if joined.count() > read.count() && joined.count() > SECRETS_KEPT_LIMIT {
            return None;
        }
        *read = joined.clone();
        Some(joined)
    }
}

impl std::fmt::Debug for Files {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Files")
            .field("env_file", &self.env.is_some())
            .field("kinds", &self.kinds.len())
            .field("secrets", &self.known().count())
            .finish()
    }
}

impl ConfigSource for Files {
    /// Reads both files and answers what they say now: the config's digest, what
    /// the config offers with this build's kinds, each ready backend made, and
    /// every credential the config names as the snapshot's secrets, in one set.
    fn snapshot(&self) -> Result<Snapshot, SourceRefused> {
        let loaded = self
            .load()
            .map_err(|refused| SourceRefused::from(&refused))?;
        let Some(known) = self.keep(loaded.secrets()) else {
            return Err(SourceRefused::because(KEYS_OVER));
        };
        let offers = Offers::knowing(loaded.config(), loaded.credentials(), &self.kinds, &known)
            .map_err(|_| SourceRefused::because(NO_ROSTER))?;
        Ok(Snapshot::of(loaded.config().digest())
            .offering(Arc::new(offers))
            .with_secrets(loaded.secrets().clone()))
    }
}

/// What a call is told of a refusal of either file: fixed words for each kind of
/// refusal, which name the file as "the config" or "the env file". Nothing a refusal
/// holds that came from a file or from the system is in them: no key, no variable's
/// name, no number of a line, no limit and no path.
impl From<&LoadRefused> for SourceRefused {
    fn from(refused: &LoadRefused) -> SourceRefused {
        use std::io::ErrorKind::{NotFound, PermissionDenied};
        SourceRefused::because(match refused {
            LoadRefused::Config(refused) => match refused {
                ConfigRefused::Unreadable(NotFound) => "the config cannot be read: it is not there",
                ConfigRefused::Unreadable(PermissionDenied) => {
                    "the config cannot be read: permission is denied"
                }
                ConfigRefused::Unreadable(_) => {
                    "the config cannot be read: the system could not read it"
                }
                ConfigRefused::NotAFile => "the config cannot be read: it is not a file",
                ConfigRefused::TooLarge { .. } => "the config is longer than a config may be",
                ConfigRefused::NotText { .. } => {
                    "the config is not text: it is UTF-8 with no ASCII control character"
                }
                ConfigRefused::HoldsSecret => {
                    "the config holds a secret: a credential is named by its variable and is \
                     never written in the file; nothing of the file is shown"
                }
                ConfigRefused::NotToml { .. } => "the config is not TOML",
                ConfigRefused::Twice { .. } => {
                    "the config gives a key twice; the key is not named here"
                }
                ConfigRefused::Key { .. } => {
                    "a key of the config is not known, is missing, or has a value that does \
                     not belong there; the key is not named here"
                }
            },
            LoadRefused::Env(refused) => match refused {
                EnvRefused::Unreadable(NotFound) => "the env file cannot be read: it is not there",
                EnvRefused::Unreadable(PermissionDenied) => {
                    "the env file cannot be read: permission is denied"
                }
                EnvRefused::Unreadable(_) => {
                    "the env file cannot be read: the system could not read it"
                }
                EnvRefused::NotAFile => "the env file cannot be read: it is not a file",
                EnvRefused::TooLarge { .. } => "the env file is longer than an env file may be",
                EnvRefused::NotText { .. } => {
                    "the env file is not text: it is UTF-8 with no ASCII control character"
                }
                EnvRefused::NotAssignment { .. } => {
                    "a line of the env file is not an assignment; the line is not shown"
                }
                EnvRefused::Twice { .. } => {
                    "a line of the env file gives a name the file has already given; the \
                     line is not shown"
                }
                EnvRefused::Value { .. } => {
                    "a value of the env file is not one value; the line is not shown"
                }
                EnvRefused::TooMany { .. } => {
                    "the env file gives more variables than an env file may"
                }
                EnvRefused::TooShort { .. } => {
                    "the env file's value for a variable the config names is too short to be \
                     a credential or to be kept out of what is written; neither is shown"
                }
                EnvRefused::NameIsAValue => {
                    "a variable the config names has a value of the env file as its name; \
                     neither is shown"
                }
                EnvRefused::Unprotected => {
                    "a value of the env file cannot be kept out of what is written, so \
                     nothing is made of the file; nothing of it is shown"
                }
            },
            LoadRefused::NoEnvFile => {
                "the config names a variable a credential is in, and no env file was given: \
                 without one nothing is known to keep out of what is written, so the config \
                 is not used; name the env file"
            }
        })
    }
}
