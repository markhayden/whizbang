//! Config (`SPEC.md` section 4; ADR 0011): the one file that chooses a backend for each
//! capability, caps what one call may spend and approves who may receive data. It names,
//! and never holds, the credentials its backends need.
//!
//! A [`Config`] is made by reading a file strictly ([`Config::read`], [`Config::parse`])
//! or is the empty one ([`Config::empty`]); there is no other way to one, its fields are
//! private, and it cannot be read from JSON or TOML by other code. Nothing is built
//! from a config yet, and it has no digest yet: the digest, the roster, the cap, the
//! policy and the settings a run records come with the later tasks of checkpoint 1B.
//! The file is TOML, read with the `toml` crate, which this crate depends on for that
//! and for nothing else.

mod read;
mod refused;

use std::collections::{BTreeMap, BTreeSet};

use crate::contract::Capability;

pub use refused::{ConfigRefused, KEY_SHOWN_LIMIT};

/// The longest config file, in bytes. A longer one is refused from its length, before
/// it is read.
pub const CONFIG_BYTES_LIMIT: u64 = 256 * 1024;

/// The most backends one capability may be given.
pub const BACKENDS_LIMIT: usize = 16;

/// The most settings one backend may be given.
pub const SETTINGS_LIMIT: usize = 32;

/// The most recipients a config may approve.
pub const RECIPIENTS_LIMIT: usize = 32;

/// The most upstreams one recipient may forward to.
pub const UPSTREAMS_LIMIT: usize = 16;

/// The longest model or account, in characters.
pub const TEXT_LIMIT: usize = 128;

/// The longest text of a setting, in characters.
pub const SETTING_TEXT_LIMIT: usize = 1024;

/// The longest address of a backend, in characters.
pub const ADDRESS_LIMIT: usize = 2048;

/// The longest name of a variable, in characters.
pub const VARIABLE_LIMIT: usize = 64;

/// The largest cap of one call: a million US dollars, in millionths.
pub const CALL_CAP_LIMIT_MICROS: u64 = 1_000_000 * 1_000_000;

/// The capabilities a config may give a backend, in the order of their names' bytes.
const CAPABILITIES: [Capability; 3] = [Capability::Fill, Capability::Judge, Capability::Segment];

/// What one config file says. Two files that say the same are equal.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Config {
    /// By the capability's name. A capability that is given no backend is not here.
    backends: BTreeMap<&'static str, Offered>,
    call_cap_micros: Option<u64>,
    recipients: BTreeMap<String, Recipient>,
}

/// The backends of one capability.
#[derive(Clone, PartialEq, Eq, Default)]
struct Offered {
    default: Option<String>,
    backends: BTreeMap<String, Backend>,
}

/// One backend, as the file gives it. What its kind makes of its settings is the kind's.
#[derive(Clone, PartialEq, Eq)]
pub struct Backend {
    kind: String,
    model: Option<String>,
    provider: Option<String>,
    url: Option<String>,
    key_env: Option<String>,
    token_env: Option<String>,
    settings: BTreeMap<String, Setting>,
}

/// The value of one setting of a backend.
#[derive(Clone, PartialEq, Eq)]
pub enum Setting {
    Text(String),
    Integer(i64),
    Boolean(bool),
}

/// A class of data a recipient may be approved for (`SPEC.md` section 10, "Egress").
/// They are in the order of their names' bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DataClass {
    DerivedImages,
    Evidence,
    Prompts,
    SourceImages,
}

impl DataClass {
    /// Every class, in the order of their names' bytes.
    pub const ALL: [DataClass; 4] = [
        DataClass::DerivedImages,
        DataClass::Evidence,
        DataClass::Prompts,
        DataClass::SourceImages,
    ];

    /// The class as the file names it.
    pub fn name(self) -> &'static str {
        match self {
            DataClass::DerivedImages => "derived_images",
            DataClass::Evidence => "evidence",
            DataClass::Prompts => "prompts",
            DataClass::SourceImages => "source_images",
        }
    }
}

/// One recipient the config approves, and what for.
#[derive(Clone, PartialEq, Eq)]
pub struct Recipient {
    origin: String,
    account: Option<String>,
    upstreams: BTreeSet<String>,
    classes: BTreeSet<DataClass>,
    probe: bool,
}

impl Config {
    /// The config of no file: no backend, no cap, no recipient.
    pub fn empty() -> Config {
        Config::default()
    }

    /// The backends the file gives a capability, each with its name, in the order of
    /// the names' bytes.
    pub fn backends(&self, capability: Capability) -> impl Iterator<Item = (&str, &Backend)> {
        self.backends
            .get(capability.name())
            .into_iter()
            .flat_map(|offered| &offered.backends)
            .map(|(name, backend)| (name.as_str(), backend))
    }

    /// The name of the backend that answers a call that names none, when the file
    /// gives the capability one.
    pub fn default_of(&self, capability: Capability) -> Option<&str> {
        self.backends.get(capability.name())?.default.as_deref()
    }

    /// The most one call may reserve, in millionths of a US dollar, when the file caps it.
    pub fn call_cap_micros(&self) -> Option<u64> {
        self.call_cap_micros
    }

    /// The recipients the file approves, each with its name, in the order of the names'
    /// bytes.
    pub fn recipients(&self) -> impl Iterator<Item = (&str, &Recipient)> {
        self.recipients
            .iter()
            .map(|(name, recipient)| (name.as_str(), recipient))
    }

    /// The name of every variable the file says a credential is in.
    pub fn variables(&self) -> BTreeSet<&str> {
        self.backends
            .values()
            .flat_map(|offered| offered.backends.values())
            .flat_map(Backend::credentials)
            .map(|(_, variable)| variable)
            .collect()
    }
}

impl Backend {
    /// The kind of backend: the name of the code that answers.
    pub fn kind(&self) -> &str {
        &self.kind
    }

    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    pub fn provider(&self) -> Option<&str> {
        self.provider.as_deref()
    }

    /// The backend's address: a scheme, a host, perhaps a port and a path, and nothing
    /// a credential could be written in.
    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    /// The credentials this backend names, each as the key that names it (`key_env`,
    /// `token_env`) and the name of the variable its value is in. The value is not
    /// here: a config holds none.
    pub fn credentials(&self) -> impl Iterator<Item = (&'static str, &str)> {
        [("key_env", &self.key_env), ("token_env", &self.token_env)]
            .into_iter()
            .filter_map(|(key, variable)| Some((key, variable.as_deref()?)))
    }

    /// The backend's own settings, each with its key, in the order of the keys' bytes.
    pub fn settings(&self) -> impl Iterator<Item = (&str, &Setting)> {
        self.settings
            .iter()
            .map(|(key, setting)| (key.as_str(), setting))
    }
}

impl Recipient {
    /// The recipient's origin: a scheme, a host and perhaps a port.
    pub fn origin(&self) -> &str {
        &self.origin
    }

    pub fn account(&self) -> Option<&str> {
        self.account.as_deref()
    }

    /// The origins the recipient may forward to, in the order of their bytes.
    pub fn upstreams(&self) -> impl Iterator<Item = &str> {
        self.upstreams.iter().map(String::as_str)
    }

    /// The classes of data the recipient may receive, in the order of their names' bytes.
    pub fn classes(&self) -> impl Iterator<Item = DataClass> {
        self.classes.iter().copied()
    }

    /// Whether the file approves probing this recipient. Unless it says so, it does not.
    pub fn may_be_probed(&self) -> bool {
        self.probe
    }
}

// A config holds no credential, and still none of its values is printed: a text a
// person wrote into a setting is not known to be harmless. Each type shows its shape.

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let backends: usize = self
            .backends
            .values()
            .map(|offered| offered.backends.len())
            .sum();
        f.debug_struct("Config")
            .field("backends", &backends)
            .field("call_cap", &self.call_cap_micros.is_some())
            .field("recipients", &self.recipients.len())
            .finish()
    }
}

impl std::fmt::Debug for Backend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Backend")
            .field("model", &self.model.is_some())
            .field("provider", &self.provider.is_some())
            .field("url", &self.url.is_some())
            .field("credentials", &self.credentials().count())
            .field("settings", &self.settings.len())
            .finish()
    }
}

impl std::fmt::Debug for Setting {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Setting::Text(_) => "Setting(text)",
            Setting::Integer(_) => "Setting(whole number)",
            Setting::Boolean(_) => "Setting(true or false)",
        })
    }
}

impl std::fmt::Debug for Recipient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Recipient")
            .field("account", &self.account.is_some())
            .field("upstreams", &self.upstreams.len())
            .field("classes", &self.classes.len())
            .field("probe", &self.probe)
            .finish()
    }
}
