//! A fake kind of backend (ADR 0011, "Kinds"): the code a config's `kind` names, for a
//! test. It sends nothing. It declares what its table says, refuses a table it does not
//! know, and keeps what it was shown, so a test can say which backend's credential a
//! kind was handed, and that it was handed no other's.

use std::sync::{Mutex, MutexGuard, PoisonError};

use whizbang_core::capability::Declaration;
use whizbang_core::config::{Kind, KindRefused, Setting, Table};
use whizbang_core::contract::{Capability, ModelIdentity};
use whizbang_core::egress::{Credential, Recipient};

/// The header a fake kind carries its key in: the kind's own constant.
pub const FAKE_KEY_HEADER: &str = "x-fake-key";

/// The header a fake kind carries its token in.
pub const FAKE_TOKEN_HEADER: &str = "x-fake-token";

/// What a fake kind says of a table that holds a setting it does not know.
pub const FAKE_UNKNOWN_SETTING: &str =
    "a fake backend takes the settings features and version, and was given another";

/// What it says of a setting it knows whose value is not a text.
pub const FAKE_SETTING_NOT_TEXT: &str = "a setting of a fake backend is a text";

/// What it says of a model, a version or a feature a backend cannot declare.
pub const FAKE_NOT_DECLARED: &str =
    "a fake backend cannot declare that model, that version or those features";

/// What it says of a table that names a credential and gives no address to bind it to.
pub const FAKE_NO_ADDRESS: &str = "a fake backend with a credential needs a url to carry it to";

/// One table a fake kind was shown, and what it made of the credentials it was handed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    pub capability: Capability,
    pub name: String,
    /// The credential made of the table's key, bound to the origin of its address.
    pub key: Option<Credential>,
    /// The same, of its token.
    pub token: Option<Credential>,
    /// What the table printed of itself.
    pub printed: String,
}

/// A kind of this name. A backend of it declares the model its table gives (`m` when it
/// gives none) of the backend that is the kind's name, at the version its setting
/// `version` gives (`1` when it gives none), with the features its setting `features`
/// lists, with commas between. A table with an address declares that origin as its
/// recipient. Any other setting is refused.
#[derive(Debug)]
pub struct FakeKind {
    name: String,
    shown: Mutex<Vec<Shown>>,
}

impl FakeKind {
    pub fn new(name: &str) -> FakeKind {
        FakeKind {
            name: name.to_owned(),
            shown: Mutex::new(Vec::new()),
        }
    }

    fn held(&self) -> MutexGuard<'_, Vec<Shown>> {
        self.shown.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Every table the kind was shown, in the order it was shown them.
    pub fn shown(&self) -> Vec<Shown> {
        self.held().clone()
    }
}

impl Kind for FakeKind {
    fn name(&self) -> &str {
        &self.name
    }

    fn declare(&self, table: &Table<'_>) -> Result<Declaration, KindRefused> {
        let backend = table.backend();
        let (mut version, mut features) = ("1", "");
        for (key, setting) in backend.settings() {
            let Setting::Text(text) = setting else {
                return Err(KindRefused::Table(FAKE_SETTING_NOT_TEXT));
            };
            match key {
                "version" => version = text,
                "features" => features = text,
                _ => return Err(KindRefused::Table(FAKE_UNKNOWN_SETTING)),
            }
        }
        let features: Vec<&str> = features.split(',').filter(|one| !one.is_empty()).collect();
        let model = ModelIdentity::new(&self.name, backend.model().unwrap_or("m"), version);
        let mut declaration = Declaration::new(model, &features)
            .map_err(|_| KindRefused::Table(FAKE_NOT_DECLARED))?;
        let carried = |value, header| match (value, backend.address()) {
            (None, _) => Ok(None),
            (Some(_), None) => Err(KindRefused::Table(FAKE_NO_ADDRESS)),
            (Some(value), Some(address)) => {
                let value: &whizbang_core::config::KeyValue = value;
                value
                    .carried_to(address, header, table.secrets())
                    .map(Some)
                    .map_err(KindRefused::Credential)
            }
        };
        let key = carried(table.key(), FAKE_KEY_HEADER)?;
        let token = carried(table.token(), FAKE_TOKEN_HEADER)?;
        if let Some(address) = backend.address() {
            declaration = declaration.sending_to(Recipient::new(address.origin().clone()));
        }
        self.held().push(Shown {
            capability: table.capability(),
            name: table.name().to_owned(),
            key,
            token,
            printed: format!("{table:?}"),
        });
        Ok(declaration)
    }
}
