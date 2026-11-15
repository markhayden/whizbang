//! The kinds of backend a build has (ADR 0011, "Kinds"): the code a backend's `kind`
//! names, registered by name as an action is.
//!
//! The core names no kind. The binary registers the kinds it links, which at this
//! checkpoint is none; a test registers a fake. A kind is shown one backend's table,
//! with the credentials that table names and no others, and makes the backend, which
//! says of itself what it declares; or it answers why it does not take the table, in
//! words that are the kind's own and are meant to hold nothing of the file.

use std::sync::Arc;

use crate::contract::{Capability, Made, Secrets, text};
use crate::egress::NotAllowed;

use super::{Backend, Handed, KeyValue};

/// The most kinds one build registers.
pub const KINDS_LIMIT: usize = 64;

/// The longest sentence of a kind that is kept, in characters.
pub const KIND_SENTENCE_LIMIT: usize = 256;

/// What is kept in place of a kind's sentence that holds a secret.
pub const KIND_SENTENCE_WITHHELD: &str =
    "its kind gave a reason that is not shown: it held a secret";

/// Why a kind does not take a backend's table: a sentence written in the kind's code,
/// or why the egress types did not make a credential. Both are meant to be fixed
/// words: the sentence is a constant in the kind's code. The type does not enforce
/// that against a kind that leaks a made text; what a config offers (`Offers::of`)
/// replaces a sentence that holds a secret of that config's credentials with fixed
/// words and cuts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KindRefused {
    /// What the table says is not what the kind takes: a setting it does not know, a
    /// value of the wrong sort, a model it has not. The sentence is the kind's.
    Table(&'static str),
    /// The credential the table names cannot be carried to the backend's address.
    Credential(NotAllowed),
}

impl KindRefused {
    /// The same refusal as it is kept (`Offers::of`): a sentence that the search for
    /// these secrets changes is replaced with [`KIND_SENTENCE_WITHHELD`], and any
    /// other is cut at [`KIND_SENTENCE_LIMIT`] characters. The look comes first,
    /// over the whole sentence; a cut takes text away and adds none, so what is left
    /// of a sentence that held no secret holds none.
    pub(super) fn kept(self, secrets: &Secrets) -> KindRefused {
        let KindRefused::Table(why) = self else {
            return self;
        };
        if secrets.redact(why) != why {
            return KindRefused::Table(KIND_SENTENCE_WITHHELD);
        }
        let cut = why.char_indices().nth(KIND_SENTENCE_LIMIT);
        KindRefused::Table(cut.map_or(why, |(at, _)| &why[..at]))
    }
}

impl std::fmt::Display for KindRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KindRefused::Table(why) => f.write_str(why),
            KindRefused::Credential(why) => {
                write!(f, "its credential cannot be carried: {why}")
            }
        }
    }
}

impl std::error::Error for KindRefused {}

/// One backend as its kind is shown it: the capability it is for, the name it is
/// offered under, its table of the config, and the credentials that table names, where
/// the env file sets them. A kind is never shown another backend's.
///
/// What `{:?}` prints is the capability and the shape of the table, as a
/// [`Backend`] and a [`Handed`] print theirs: no name, no value.
pub struct Table<'a> {
    pub(super) capability: Capability,
    pub(super) name: &'a str,
    pub(super) backend: &'a Backend,
    pub(super) handed: &'a Handed,
    pub(super) secrets: &'a Secrets,
}

impl Table<'_> {
    /// The capability the backend is offered for.
    pub fn capability(&self) -> Capability {
        self.capability
    }

    /// The name a call chooses the backend by.
    pub fn name(&self) -> &str {
        self.name
    }

    /// The backend's table: its model, its provider, its address and its settings.
    pub fn backend(&self) -> &Backend {
        self.backend
    }

    /// The credential the table's `key_env` names.
    pub fn key(&self) -> Option<&KeyValue> {
        self.handed.key()
    }

    /// The credential the table's `token_env` names.
    pub fn token(&self) -> Option<&KeyValue> {
        self.handed.token()
    }

    /// The secrets a credential is made with ([`KeyValue::carried_to`]): the
    /// executor's, which are every credential the config names that has a value.
    pub fn secrets(&self) -> &Secrets {
        self.secrets
    }
}

impl std::fmt::Debug for Table<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Table")
            .field("capability", &self.capability)
            .field("backend", self.backend)
            .field("handed", self.handed)
            .finish()
    }
}

/// One kind of backend: the code a table's `kind` names.
pub trait Kind: Send + Sync {
    /// The name a config says it by. A name, as an action's is. The same on every call.
    fn name(&self) -> &str;

    /// Makes the backend this table describes: what answers the requests of the runs
    /// admitted under the config the table is of, and of no other run. What the
    /// backend declares, its model, its features, its price and who receives its
    /// requests, is what the backend itself says ([`Made::declaration`]), so what a
    /// config offers and what a run is handed are one thing. It is made from the
    /// table and from nothing else: nothing is sent, and nothing is read from a
    /// disk. It answers the capability the table is under.
    ///
    /// A kind that needs a credential makes it with [`KeyValue::carried_to`], bound to
    /// the origin of the address it sends to, under a header that is the kind's own
    /// constant.
    fn make(&self, table: &Table<'_>) -> Result<Made, KindRefused>;
}

/// Why a kind was not registered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KindNotRegistered {
    /// What the kind gives as its name is not a name. It is not shown.
    Name,
    /// A kind of this name is registered already.
    Duplicate { name: String },
    /// The build has as many kinds as it may.
    TooMany { limit: usize },
}

impl std::fmt::Display for KindNotRegistered {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KindNotRegistered::Name => f.write_str(
                "a kind's name is a lower-case letter, then lower-case letters, digits, _ or \
                 -, 64 characters at most; what was given is not shown",
            ),
            KindNotRegistered::Duplicate { name } => {
                write!(f, "a kind named {name} is registered already")
            }
            KindNotRegistered::TooMany { limit } => {
                write!(f, "a build registers {limit} kinds at most")
            }
        }
    }
}

impl std::error::Error for KindNotRegistered {}

/// The kinds of one build, by name. What `{:?}` prints is how many there are.
#[derive(Clone, Default)]
pub struct Kinds {
    kinds: Vec<Arc<dyn Kind>>,
}

impl Kinds {
    /// No kind: every backend of a config is then of a kind the build does not have.
    pub fn new() -> Kinds {
        Kinds::default()
    }

    /// Adds a kind. A name is registered once, and is a name as an action's is; a
    /// kind that is refused leaves the kinds as they were.
    pub fn register(&mut self, kind: Arc<dyn Kind>) -> Result<(), KindNotRegistered> {
        let name = kind.name();
        if text::action_name(name).is_err() {
            return Err(KindNotRegistered::Name);
        }
        if self.get(name).is_some() {
            return Err(KindNotRegistered::Duplicate {
                name: name.to_owned(),
            });
        }
        if self.kinds.len() >= KINDS_LIMIT {
            return Err(KindNotRegistered::TooMany { limit: KINDS_LIMIT });
        }
        self.kinds.push(kind);
        Ok(())
    }

    /// The kind of this name.
    pub fn get(&self, name: &str) -> Option<&Arc<dyn Kind>> {
        self.kinds.iter().find(|kind| kind.name() == name)
    }

    pub fn len(&self) -> usize {
        self.kinds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }
}

impl std::fmt::Debug for Kinds {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Kinds({})", self.kinds.len())
    }
}
