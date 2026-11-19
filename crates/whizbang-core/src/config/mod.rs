//! Config (`SPEC.md` section 4; ADR 0011): the one file that chooses a backend for each
//! capability, caps what one call may spend and approves who may receive data. It names,
//! and never holds, the credentials its backends need.
//!
//! A [`Config`] is made by reading a file strictly ([`Config::read`], [`Config::parse`])
//! or is the empty one ([`Config::empty`]); there is no other way to one, its fields are
//! private, and it cannot be read from JSON or TOML by other code. Its digest
//! ([`Config::digest`]) is of the settings and not of the file's bytes. The file is
//! TOML, read with the `toml` crate, which this crate depends on for that and for
//! nothing else.
//!
//! From a config, the credentials of its env file and the kinds a build registers
//! ([`Kinds`]), [`Offers::of`] makes what the config offers: a roster of the backends
//! that are ready, and each that is not with its reason. [`Config::settings_of`] gives
//! the settings a run records, and [`Loaded::of_files`] reads both files again each
//! time it is asked. What a config offers is what a snapshot of the config holds as
//! its backends ([`Offered`](crate::contract::Offered), ADR 0015), and a config makes
//! the settings of a snapshot's runs ([`Settings`](crate::contract::Settings)).
//! [`Files`] is the source an executor asks: it reads both files on each ask and
//! answers a snapshot of them, with the cap, the policy and the settings the config
//! gives, or a refusal in fixed words.

mod env;
mod kinds;
mod offers;
mod read;
mod refused;
mod settings;
mod source;

use std::collections::{BTreeMap, BTreeSet};

use crate::egress::{Account, Address, Classes, DataClass, Origin, Recipient};

use crate::contract::Capability;
use crate::identity::{Digest, Encoder, Fields};

pub use env::{
    Credentials, ENV_BYTES_LIMIT, ENV_VALUE_LIMIT, ENV_VARIABLES_LIMIT, EnvFile, EnvRefused,
    Handed, KeyValue, LoadRefused, Loaded,
};
pub use kinds::{
    KIND_SENTENCE_LIMIT, KIND_SENTENCE_WITHHELD, KINDS_LIMIT, Kind, KindNotRegistered, KindRefused,
    Kinds, Table,
};
pub use offers::{NotReady, Offers, Unanswered};
pub use refused::{ConfigRefused, KEY_SHOWN_LIMIT};
pub use settings::SettingsRefused;
pub use source::Files;

/// The longest config file, in bytes. A longer one is refused from its length, before
/// it is read.
pub const CONFIG_BYTES_LIMIT: u64 = 256 * 1024;

/// The most backends one capability may be given.
pub const BACKENDS_LIMIT: usize = 16;

/// The most settings one backend may be given.
pub const SETTINGS_LIMIT: usize = 32;

/// The most recipients a config may approve.
pub const RECIPIENTS_LIMIT: usize = 32;

/// The longest model, in characters.
pub const TEXT_LIMIT: usize = 128;

/// The longest text of a setting, in characters.
pub const SETTING_TEXT_LIMIT: usize = 1024;

/// The longest name of a variable, in characters.
pub const VARIABLE_LIMIT: usize = 64;

/// The largest cap of one call: a million US dollars, in millionths.
pub const CALL_CAP_LIMIT_MICROS: u64 = 1_000_000 * 1_000_000;

/// The most bytes of the models directory's path, as a config names it.
pub const MODELS_DIR_LIMIT: usize = 1024;

/// The most tool files `[tools] paths` lists.
pub const TOOL_PATHS_LIMIT: usize = 16;

/// The least rate of a download, in kibibytes a second, when no config gives one.
pub const DOWNLOAD_LEAST_KIB_PER_SECOND: u64 = 256;

/// The most a config may give as the least rate of a download: a gibibyte a second.
pub const DOWNLOAD_LEAST_KIB_LIMIT: u64 = 1024 * 1024;

/// The most origins the config may name a download's token for (`[[models.tokens]]`).
pub const MODEL_TOKENS_LIMIT: usize = 8;

/// The one origin a token of the Hub's command-line sign-in may be named for, and so
/// the one it is ever sent to (plan 1C, D12). A config that names the sign-in for any
/// other origin is refused.
pub const SIGN_IN_ORIGIN: &str = "https://huggingface.co";

/// What a config says of where a download's token is: `sign_in = "huggingface"`, or
/// `key_env`, a variable of the env file (ADR 0011, "A download's token").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenFrom {
    /// The token the Hub's command-line tool keeps for its sign-in, which is read only
    /// when a download from [`SIGN_IN_ORIGIN`] needs it.
    SignIn,
    /// The variable of the env file it is in, read as every key is.
    Variable(String),
}

/// One origin's token, as the config names it: never the token itself.
#[derive(Clone, PartialEq, Eq)]
pub struct ModelToken {
    pub(crate) origin: Origin,
    pub(crate) from: TokenFrom,
}

impl ModelToken {
    /// The origin a download from which carries the token.
    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    /// Where the token is.
    pub fn from(&self) -> &TokenFrom {
        &self.from
    }
}

impl std::fmt::Debug for ModelToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let from = match self.from {
            TokenFrom::SignIn => "sign-in",
            TokenFrom::Variable(_) => "variable",
        };
        f.debug_struct("ModelToken")
            .field("origin", &self.origin)
            .field("from", &from)
            .finish()
    }
}

/// The capabilities, in the order of their names' bytes: the order a config's digest
/// takes them in.
const CAPABILITIES: [Capability; 3] = [Capability::Fill, Capability::Judge, Capability::Segment];

/// What one config file says. Two files that say the same are equal and have one digest.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Config {
    /// By the capability's name. A capability that is given no backend is not here.
    backends: BTreeMap<&'static str, Offered>,
    call_cap_micros: Option<u64>,
    recipients: BTreeMap<String, Approval>,
    /// `[models] dir`, as it is written.
    models_dir: Option<String>,
    /// `[models] download_least_kib_per_second`.
    download_least_kib: Option<u64>,
    /// `[[models.tokens]]`, each origin once, in the order of their texts.
    model_tokens: Vec<ModelToken>,
    /// `[cache]`: whether judges' decisions are cached (`SPEC.md` section 9). Not in
    /// the digest: whether a run may answer from the cache says nothing of what a run
    /// means, and the cache's key holds the digest.
    decisions_cached: bool,
    /// `[tools] paths` (plan 2C, task X1; ADR 0032): the tool files, each an absolute
    /// path as it is written. Not in the digest: what a run of a tool file ran is the
    /// file's digest, which the run records.
    tool_paths: Vec<String>,
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
    url: Option<Address>,
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

/// One recipient the config approves, and what for: the recipient as the egress types
/// make one (ADR 0012), the classes of data it may receive, and whether it may be
/// probed.
#[derive(Clone, PartialEq, Eq)]
pub struct Approval {
    recipient: Recipient,
    /// The account's whole text, for the digest: an `Account` shows only its start.
    account: Option<String>,
    classes: Classes,
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
    pub fn recipients(&self) -> impl Iterator<Item = (&str, &Approval)> {
        self.recipients
            .iter()
            .map(|(name, recipient)| (name.as_str(), recipient))
    }

    /// Whether judges' decisions are cached: the file has a table `[cache]` whose
    /// `decisions` is not false (plan 2C, task M1; ADR 0031). Off without one.
    pub fn decisions_cached(&self) -> bool {
        self.decisions_cached
    }

    /// The tool files the file lists, `[tools] paths`, in its order: none without it.
    pub fn tool_paths(&self) -> impl Iterator<Item = &std::path::Path> {
        self.tool_paths.iter().map(std::path::Path::new)
    }

    /// Where the models directory is, when the file says: `[models] dir`, an absolute
    /// path as it is written. A command's flag is asked first, and the platform's
    /// data folder serves when neither says (ADR 0011, "What the file holds").
    pub fn models_dir(&self) -> Option<&std::path::Path> {
        self.models_dir.as_deref().map(std::path::Path::new)
    }

    /// The least rate of a download in kibibytes a second, when the file gives one:
    /// `[models] download_least_kib_per_second`. Whoever makes the download client
    /// gives it this, or [`DOWNLOAD_LEAST_KIB_PER_SECOND`].
    pub fn download_least_kib_per_second(&self) -> Option<u64> {
        self.download_least_kib
    }

    /// Where the token of a download from this origin is, when the file names it:
    /// `[[models.tokens]]` (ADR 0011, "A download's token"). None for an origin it
    /// does not name.
    pub fn model_token(&self, origin: &Origin) -> Option<&TokenFrom> {
        self.model_tokens
            .iter()
            .find(|token| &token.origin == origin)
            .map(|token| &token.from)
    }

    /// Every download's token the file names, in the order of their origins' texts.
    pub fn model_tokens(&self) -> &[ModelToken] {
        &self.model_tokens
    }

    /// The name of every variable the file says a credential is in: a backend's, and
    /// a download's token's.
    pub fn variables(&self) -> BTreeSet<&str> {
        let tokens = self
            .model_tokens
            .iter()
            .filter_map(|token| match &token.from {
                TokenFrom::Variable(name) => Some(name.as_str()),
                TokenFrom::SignIn => None,
            });
        self.backends
            .values()
            .flat_map(|offered| offered.backends.values())
            .flat_map(Backend::credentials)
            .map(|(_, variable)| variable)
            .chain(tokens)
            .collect()
    }

    /// The digest of what the file says, kind `config/1`, in the encoding ADR 0001 gives
    /// every identity. ADR 0011 states the fields in full; in short, each list below is
    /// its count and then its entries in the order of their names' bytes, and a thing
    /// that may be absent is 0, or 1 and the thing:
    ///
    /// 1. the capabilities that are given a backend, each as its name, its default
    ///    (may be absent), and its backends, each as its name, its kind, its model,
    ///    provider, address, key variable and token variable (each may be absent), and
    ///    its settings, each as its key and its value: 1 and a whole number, 2 and a
    ///    boolean, or 3 and a text;
    /// 2. the cap of one call in millionths (may be absent);
    /// 3. the recipients, each as its name, its origin, its account (may be absent),
    ///    its upstreams, its classes, and whether it may be probed;
    ///
    /// The `models` table is not in it, whatever it gives: where the models directory
    /// is and the least rate of a download say nothing of what a run means.
    ///
    /// No value of a credential is in it: a config holds none.
    pub fn digest(&self) -> Digest {
        let mut fields = Encoder::new("config/1");
        fields.unsigned(self.backends.len() as u64);
        for capability in CAPABILITIES {
            let Some(offered) = self.backends.get(capability.name()) else {
                continue;
            };
            fields.bytes(capability.name().as_bytes());
            optional(&mut fields, offered.default.as_deref());
            fields.unsigned(offered.backends.len() as u64);
            for (name, backend) in &offered.backends {
                fields.bytes(name.as_bytes());
                fields.bytes(backend.kind.as_bytes());
                // The address as its origin writes itself, and then its path as given.
                let url = backend.url();
                for text in [
                    &backend.model,
                    &backend.provider,
                    &url,
                    &backend.key_env,
                    &backend.token_env,
                ] {
                    optional(&mut fields, text.as_deref());
                }
                fields.unsigned(backend.settings.len() as u64);
                for (key, setting) in &backend.settings {
                    fields.bytes(key.as_bytes());
                    match setting {
                        Setting::Integer(number) => {
                            fields.unsigned(1);
                            fields.unsigned(u64::from_be_bytes(number.to_be_bytes()));
                        }
                        Setting::Boolean(boolean) => {
                            fields.unsigned(2);
                            fields.unsigned(u64::from(*boolean));
                        }
                        Setting::Text(text) => {
                            fields.unsigned(3);
                            fields.bytes(text.as_bytes());
                        }
                    }
                }
            }
        }
        match self.call_cap_micros {
            Some(micros) => {
                fields.unsigned(1);
                fields.unsigned(micros);
            }
            None => fields.unsigned(0),
        }
        fields.unsigned(self.recipients.len() as u64);
        for (name, recipient) in &self.recipients {
            fields.bytes(name.as_bytes());
            // An origin as an origin writes itself: lower case, and no port when it is
            // the scheme's own. The upstreams are kept in the order of that text.
            fields.bytes(recipient.origin().to_string().as_bytes());
            optional(&mut fields, recipient.account.as_deref());
            fields.unsigned(recipient.upstreams().len() as u64);
            for upstream in recipient.upstreams() {
                fields.bytes(upstream.to_string().as_bytes());
            }
            let classes: BTreeSet<&str> = recipient.classes.iter().map(DataClass::name).collect();
            fields.unsigned(classes.len() as u64);
            for class in classes {
                fields.bytes(class.as_bytes());
            }
            fields.unsigned(u64::from(recipient.probe));
        }
        // The `models` table is in no digest: where the models directory is and how
        // slow a download may be are not part of what a run means, and a run's model
        // is the identity of its installation, wherever its folder is (ADR 0011, "The
        // digest", amended with task N3b).
        fields.finish()
    }
}

/// A text that may be absent: 0, or 1 and the text.
fn optional(fields: &mut Encoder, text: Option<&str>) {
    match text {
        Some(text) => {
            fields.unsigned(1);
            fields.bytes(text.as_bytes());
        }
        None => fields.unsigned(0),
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

    /// The backend's address: its origin and the path after it, with nothing a
    /// credential could be written in.
    pub fn address(&self) -> Option<&Address> {
        self.url.as_ref()
    }

    /// The backend's address as a text: its origin as an origin is written, and the
    /// path as the file gave it.
    pub fn url(&self) -> Option<String> {
        let address = self.url.as_ref()?;
        Some(format!("{}{}", address.origin(), address.rest()))
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

impl Approval {
    /// The recipient that is approved.
    pub fn recipient(&self) -> &Recipient {
        &self.recipient
    }

    /// The recipient's origin: a scheme, a host in lower case, and a port.
    pub fn origin(&self) -> &Origin {
        self.recipient.origin()
    }

    pub fn account(&self) -> Option<&Account> {
        self.recipient.account()
    }

    /// The origins the recipient may forward to, in the order of their text.
    pub fn upstreams(&self) -> &[Origin] {
        self.recipient.upstreams()
    }

    /// The classes of data the recipient may receive.
    pub fn classes(&self) -> Classes {
        self.classes
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
            .field("models_dir", &self.models_dir.is_some())
            .field("download_least", &self.download_least_kib.is_some())
            .field("model_tokens", &self.model_tokens.len())
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

impl std::fmt::Debug for Approval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Approval")
            .field("account", &self.account().is_some())
            .field("upstreams", &self.upstreams().len())
            .field("classes", &self.classes.iter().count())
            .field("probe", &self.probe)
            .finish()
    }
}
