//! The kinds of backend a build has (ADR 0011, "Kinds"): the code a backend's `kind`
//! names, registered by name as an action is.
//!
//! The core names no kind. The binary registers the kinds it links, which at this
//! checkpoint is none; a test registers a fake. A kind is shown one backend's table,
//! with the credentials that table names and no others, and answers what the backend
//! declares, or why it does not take the table, in words that are the kind's own and
//! hold nothing of the file.

use std::sync::Arc;

use crate::capability::Declaration;
use crate::contract::{Capability, Secrets, text};
use crate::egress::NotAllowed;

use super::{Backend, Handed, KeyValue};

/// The most kinds one build registers.
pub const KINDS_LIMIT: usize = 64;

/// Why a kind does not take a backend's table. Both are fixed words: the first is a
/// sentence written in the kind's code, the second is why the egress types did not make
/// a credential. Neither can hold a value of a config or of an env file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KindRefused {
    /// What the table says is not what the kind takes: a setting it does not know, a
    /// value of the wrong sort, a model it has not. The sentence is the kind's.
    Table(&'static str),
    /// The credential the table names cannot be carried to the backend's address.
    Credential(NotAllowed),
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

    /// What a backend with this table declares: its model, its features, its price
    /// and who receives its requests. Answered from the table and from nothing else:
    /// nothing is sent, and nothing is read from a disk.
    ///
    /// A kind that needs a credential makes it with [`KeyValue::carried_to`], bound to
    /// the origin of the address it sends to, under a header that is the kind's own
    /// constant.
    fn declare(&self, table: &Table<'_>) -> Result<Declaration, KindRefused>;
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
